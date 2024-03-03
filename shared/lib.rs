use clap::Parser;
use colored::Colorize;
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use path_clean::PathClean;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use rayon::prelude::*;
use regex::Regex;
use rusqlite::{params, Connection, Result};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::fs::metadata;
use std::fs::File;
use std::io::Read;
use std::io::{self, Write};
use std::io::{BufRead, Error};
use std::io::{BufReader, ErrorKind};
use std::num::ParseFloatError;
use std::path::Path;
use std::process::Output;
use std::process::Stdio;
use std::str;
use std::str::FromStr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::vec;
use std::{env, process::Command, string::String, vec::Vec};
use threadpool::ThreadPool;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
pub struct Progress {
    pub id: String,
    fps: u64,
    frame: u64,
    frames: f32,
    percentage: f32,
    eta: String,
    size: f32,
    current_file_count: u64,
    total_files: u64,
    current_file_name: String,
}

#[derive(Serialize, Debug)]
pub struct ProgressScan {
    count: u64,
    total: u64,
}

// Global variable to store the latest FFmpeg output
lazy_static! {
    pub static ref WEB_TASK_ID_STATIC: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    pub static ref WEB_PAGE_STATIC: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    pub static ref WEB_FPS_STATIC: Arc<Mutex<u64>> = Arc::new(Mutex::new(u64::MAX));
    pub static ref WEB_CURRENT_FRAME_STATIC: Arc<Mutex<u64>> = Arc::new(Mutex::new(u64::MAX));
    pub static ref WEB_TOTAL_FRAME_STATIC: Arc<Mutex<f32>> = Arc::new(Mutex::new(f32::MAX));
    pub static ref WEB_EXPECTED_SIZE_STATIC: Arc<Mutex<f32>> = Arc::new(Mutex::new(f32::MAX));
    pub static ref WEB_CURRENT_FILE_STATIC: Arc<Mutex<u64>> = Arc::new(Mutex::new(u64::MAX));
    pub static ref WEB_TOTAL_FILES_STATIC: Arc<Mutex<u64>> = Arc::new(Mutex::new(u64::MAX));
    pub static ref WEB_CURRENT_FILE_NAME_STATIC: Arc<Mutex<String>> =
        Arc::new(Mutex::new(String::new()));
    pub static ref WEB_SCAN_COUNT_STATIC: Arc<Mutex<u64>> = Arc::new(Mutex::new(u64::MAX));
    pub static ref WEB_SCAN_TOTAL_STATIC: Arc<Mutex<u64>> = Arc::new(Mutex::new(u64::MAX));
}

// Define a struct to hold the progress of each transcode task
pub struct TranscodeProgress {
    pub task_id: String,
    pub current_frame: u64,
    pub total_frame: f32,
    pub fps: u64,
}

#[derive(Parser, Serialize, Deserialize, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[clap(
    name = " TransRustica",
    author = "BankaiNoJutsu <lbegert@gmail.com>",
    about = "Transcoding, Rust, FFMPEG, VMAF, Chunking ",
    long_about = None
)]
pub struct Args {
    /// input video path folder path (\\... or /... or C:\...)
    #[clap(short = 'i', long, value_parser = input_validation)]
    pub inputpath: String,

    /// output video path folder path (\\... or /... or C:\...)
    /// (default: .)
    #[clap(short = 'o', long, default_value = ".")]
    pub outputpath: String,

    /// VMAF target value
    /// (default: 97)
    #[clap(short = 'v', long, default_value = "97")]
    pub vmaf: i32,

    /// Encoder to use
    /// (default: libx265)
    /// (possible values: libx265, av1, libsvtav1, hevc_nvenc, hevc_qsv, av1_qsv)
    #[clap(short = 'e', long, default_value = "libx265")]
    pub encoder: String,

    // output folder
    #[clap(short = 'o', long, default_value = ".")]
    pub output_folder: String,

    // show output crf search
    #[clap(long)]
    pub verbose: bool,

    /// Which mode to use for processing
    /// (default: default)
    /// (possible values: default, chunked)
    // Mode
    #[clap(short = 'm', long, default_value = "default", value_parser = possible_mode_values)]
    pub mode: String,

    /// Which vmaf pool method to use
    /// (default: mean)
    /// (possible values: min, harmonic_mean, mean)
    // Mode
    #[clap(short = 'p', long, default_value = "mean", value_parser = possible_pool_values)]
    pub vmaf_pool: String,

    // vmaf threads
    #[clap(short = 't', long, default_value = "2", value_parser = vmaf_thread_input_validation)]
    pub vmaf_threads: String,

    /// Every n frame to subsample in the vmaf calculation
    /// (default: 1)
    #[clap(short = 'S', long, default_value = "1", value_parser = vmaf_subsample_input_validation)]
    pub vmaf_subsample: String,

    /// Pixel format to use
    /// (default: yuv420p10le)
    #[clap(long, default_value = "yuv420p10le")]
    pub pix_fmt: String,

    /// Max CRF value
    /// (default: 28)
    /// (possible values: 0-51)
    #[clap(long, default_value = "28")]
    pub max_crf: String,

    /// Sample every Nth minute
    /// (default: 3m)
    #[clap(long, default_value = "3m")]
    pub sample_every: String,

    /// Params for ab-av1
    /// (default: limit-sao,bframes=8,psy-rd=1,aq-mode=3)
    #[clap(
        long,
        default_value = "x265-params=limit-sao,bframes=8,psy-rd=1,aq-mode=3"
    )]
    pub params_ab_av1: String,

    /// Params for x265
    /// (default: -x265-params limit-sao:bframes=8:psy-rd=1:aq-mode=3)
    #[clap(
        long,
        default_value = "-x265-params limit-sao:bframes=8:psy-rd=1:aq-mode=3"
    )]
    pub params_x265: String,

    /// Preset for x265
    /// (default: slow)
    /// (possible values: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow, placebo)
    #[clap(long, default_value = "slow")]
    pub preset_x265: String,

    /// Preset for av1
    /// (default: 8)
    #[clap(long, default_value = "8")]
    pub preset_av1: String,

    /// Preset for hevc_nvenc
    /// (default: p7)
    #[clap(long, default_value = "p7")]
    pub preset_hevc_nvenc: String,

    /// Params for hevc_nvenc
    /// (default: -rc-lookahead 100 -b_ref_mode each -tune hq)
    #[clap(long, default_value = "-rc-lookahead 100 -b_ref_mode each -tune hq")]
    pub params_hevc_nvenc: String,

    /// Preset for hevc_qsv
    /// (default: veryslow)
    #[clap(long, default_value = "veryslow")]
    pub preset_hevc_qsv: String,

    /// Preset for av1_qsv
    /// (default: veryslow)
    #[clap(long, default_value = "1")]
    pub preset_av1_qsv: String,

    /// Params for hevc_qsv
    /// (default: -init_hw_device qsv=intel,child_device=0 -b_strategy 1 -look_ahead 1 -async_depth 100)
    #[clap(
        long,
        default_value = "-init_hw_device qsv=intel,child_device=0 -b_strategy 1 -look_ahead 1 -async_depth 100"
    )]
    pub params_hevc_qsv: String,

    /// Params for av1_qsv
    /// (default: -init_hw_device qsv=intel,child_device=0 -g 256 -b_strategy 1 -look_ahead 1 -async_depth 100)
    #[clap(
        long,
        default_value = "-init_hw_device qsv=intel,child_device=0 -b_strategy 1 -look_ahead 1 -async_depth 100"
    )]
    pub params_av1_qsv: String,

    /// Preset for libsvtav1
    /// (default: 5)
    /// (possible values: -2 - 13)
    #[clap(long, default_value = "5")]
    pub preset_libsvtav1: String,

    /// Params for libsvtav1
    /// (default: )
    #[clap(long, default_value = "")]
    pub params_libsvtav1: String,

    /// Preset for libaom-av1
    /// (default: 4)
    #[clap(long, default_value = "4")]
    pub preset_libaom_av1: String,

    /// Params for libaom-av1
    /// (default: )
    #[clap(long, default_value = "")]
    pub params_libaom_av1: String,

    /// Scene split minimum seconds
    /// (default: 2)
    #[clap(short = 's', long, default_value = "2")]
    pub scene_split_min: f32,

    /// Task ID
    /// (default: "")
    #[clap(short = 'd', long, default_value = "")]
    pub task_id: String,
}

// This function parses the frame number from the ffmpeg output
fn parse_frame_from_output(output: &str) -> Option<u64> {
    // The regex pattern for frame number in ffmpeg output
    let re = Regex::new(r"frame=\s*(\d+)").unwrap();
    if let Some(caps) = re.captures(output) {
        let frame_str = &caps[1];
        return Some(frame_str.parse().unwrap());
    }
    None
}

fn parse_fps_from_output(output: &str) -> Option<u64> {
    // The regex pattern for frame number in ffmpeg output
    let re = Regex::new(r"fps=\s*(\d+)").unwrap();
    if let Some(caps) = re.captures(output) {
        let fps_str = &caps[1];
        return Some(fps_str.parse().unwrap());
    }
    None
}

fn get_file_size(file_path: &str) -> Result<f32, ParseFloatError> {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=size")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute ffprobe");

    let size_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    size_str.parse::<f32>()
}

pub fn get_framecount(file_path: &str) -> Result<f32, ParseFloatError> {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-v")
        .arg("error")
        .arg("-count_frames")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=nb_read_frames")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute ffprobe");

    let framecount_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    framecount_str.parse::<f32>()
}

pub fn get_framecount_tag(file_path: &str) -> Result<f32, ParseFloatError> {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream_tags=NUMBER_OF_FRAMES-eng")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute ffprobe");

    let framecount_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    framecount_str.parse::<f32>()
}

pub fn get_framecount_metadata(file_path: &str) -> Result<f32, ParseFloatError> {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream_tags=NUMBER_OF_FRAMES")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute ffprobe");

    let framecount_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    framecount_str.parse::<f32>()
}

// Use ".\ffmpeg.exe -i file -map 0:v:0 -c copy -f null -"
pub fn get_framecount_ffmpeg(file_path: &str) -> Result<f32, ParseFloatError> {
    let mut ffmpeg_command = Command::new("ffmpeg")
        .arg("-i")
        .arg(file_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-c")
        .arg("copy")
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute ffmpeg");

    // Read the output asynchronously
    let stderr = ffmpeg_command.stderr.take().unwrap();
    let mut stderr_reader = io::BufReader::new(stderr);
    let mut stderr_output = String::new();

    stderr_reader.read_to_string(&mut stderr_output).unwrap();

    // Wait for the ffmpeg command to finish
    ffmpeg_command.wait().expect("failed to wait for ffmpeg");

    // Process the output and return the frame count "frame=64936 fps=2256 q=-1.0 Lsize=N/A time=00:45:08.24 bitrate=N/A speed=94.1x"
    // Get the frame count from the output, after the run is finished, on the last line
    let framecount_str = stderr_output
        .lines()
        .last()
        .unwrap_or(&"")
        .split("frame=")
        .collect::<Vec<&str>>()[1]
        .split(" ")
        .collect::<Vec<&str>>()[0];
    framecount_str.parse::<f32>()
}

// Function to get the audio codec and channels for each audio stream
pub fn get_audio_details(file_path: &str) -> Result<Vec<(String, String)>, ParseFloatError> {
    // Count the number of audio streams
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute ffprobe");

    let audio_stream = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let audio_streams_count = audio_stream.lines().count() as i32;

    let mut audio_details = Vec::new();

    for i in 0..audio_streams_count {
        let output = Command::new("ffprobe")
            .arg("-i")
            .arg(file_path)
            .arg("-v")
            .arg("error")
            .arg("-select_streams")
            .arg(format!("a:{}", i))
            .arg("-show_entries")
            .arg("stream=codec_name,channels,channel_layout")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .output()
            .expect("failed to execute ffprobe");

        let audio_codec_channels_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let audio_codec_channels: Vec<&str> = audio_codec_channels_str.split_whitespace().collect();
        let audio_codec = audio_codec_channels[0].to_string();
        let audio_channels = audio_codec_channels[1].to_string();
        audio_details.push((audio_codec, audio_channels));
    }

    Ok(audio_details)
}

// Function to get the video codec and resolution for each video stream
pub fn get_video_details(
    file_path: &str,
) -> Result<Vec<(String, String, String)>, ParseFloatError> {
    // Count the number of video streams
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute ffprobe");

    let video_stream = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let video_streams_count = video_stream.lines().count() as i32;

    let mut video_details = Vec::new();

    for i in 0..video_streams_count {
        let output = Command::new("ffprobe")
            .arg("-i")
            .arg(file_path)
            .arg("-v")
            .arg("error")
            .arg("-select_streams")
            .arg(format!("v:{}", i))
            .arg("-show_entries")
            .arg("stream=codec_name,width,height")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .output()
            .expect("failed to execute ffprobe");

        let video_codec_resolution_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let video_codec_resolution: Vec<&str> =
            video_codec_resolution_str.split_whitespace().collect();
        let video_codec = video_codec_resolution[0].to_string();
        let video_resolution_width = video_codec_resolution[1].to_string();
        let video_resolution_height = video_codec_resolution[2].to_string();
        video_details.push((video_codec, video_resolution_width, video_resolution_height));
    }

    Ok(video_details)
}

pub fn set_output_folder_filename(
    file: &str,
    encoder: &str,
    final_vmaf: &i32,
    target_crf: &str,
    output_folder: &str,
) -> String {
    // trim target_crf
    let trim_target_crf = target_crf.trim();

    let stem = Path::new(&file)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let extension = Path::new(&file)
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // get the output folder from the arguments
    let output_folder = &output_folder;

    // add the codec and the vmaf score to the output filename
    let output_filename = format!(
        "{}.{}.vmaf{}.crf{}.{}",
        stem, encoder, final_vmaf, trim_target_crf, extension
    );

    // return the output folder and filename
    return output_folder.to_owned().to_owned() + "\\" + &output_filename;
}

pub fn set_output_folder_filename_audio(file: &str, output_folder: &str) -> String {
    let stem = Path::new(&file)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let extension = Path::new(&file)
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // get the output folder from the arguments
    let output_folder = &output_folder;

    // add the codec and the vmaf score to the output filename
    let output_filename = format!("{}.{}", stem, extension);

    // return the output folder and filename
    return output_folder.to_owned().to_owned() + "\\" + &output_filename;
}

fn run_ffmpeg_map_metadata(file: &str) -> String {
    let mut cmd = Command::new("ffprobe");

    // Get audio stream count
    let audio_output = execute_ffprobe(&mut cmd, file, "a");
    let audio_streams_count = count_streams(&audio_output);

    // Get subtitle stream count
    let subtitle_output = execute_ffprobe(&mut cmd, file, "s");
    let subtitle_streams_count = count_streams(&subtitle_output);

    let mut map_metadata_builder = StringBuilder::new();

    // Build map_metadata arguments for audio streams
    build_map_metadata_arguments(&mut map_metadata_builder, "a", audio_streams_count);

    // Build map_metadata arguments for subtitle streams
    build_map_metadata_arguments(&mut map_metadata_builder, "s", subtitle_streams_count);

    map_metadata_builder.to_string()
}

fn execute_ffprobe(cmd: &mut Command, file: &str, stream_type: &str) -> Output {
    cmd.args(&[
        "-i",
        file,
        "-v",
        "error",
        "-select_streams",
        stream_type,
        "-show_entries",
        "stream=codec_name",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
    ])
    .output()
    .unwrap_or_else(|error| {
        panic!("Failed to execute process: {}", error);
    })
}

fn count_streams(output: &Output) -> i32 {
    let stream_output = str::from_utf8(&output.stdout).unwrap().trim();
    stream_output.lines().count() as i32
}

fn build_map_metadata_arguments(builder: &mut StringBuilder, stream_type: &str, stream_count: i32) {
    for i in 0..stream_count {
        builder.push_str("-map_metadata:s:");
        builder.push_str(stream_type);
        builder.push(':');
        builder.push_str(&i.to_string());
        builder.push(' ');
        builder.push_str("0:s:");
        builder.push_str(stream_type);
        builder.push(':');
        builder.push_str(&i.to_string());
        builder.push(' ');
    }
}

pub fn format_timecode(timecode: &f32) -> String {
    // format the scene change from seconds to a timecode like 00:00:00.000
    let timecode = timecode.to_string();
    let timecode = timecode.split(".").collect::<Vec<&str>>();
    let timecode = timecode[0].parse::<i32>().unwrap();
    let hours = timecode / 3600;
    let minutes = (timecode % 3600) / 60;
    let seconds = timecode % 60;
    let miliseconds = timecode % 1000;
    let timecode = format!(
        "{:02}:{:02}:{:02}.{:03}",
        hours, minutes, seconds, miliseconds
    );
    timecode
}

fn parse_vmaf_score(output: &Output) -> Option<f32> {
    let output_str_stderr = String::from_utf8_lossy(&output.stderr);
    for line in output_str_stderr.lines() {
        if line.contains("VMAF score:") {
            return line.split("VMAF score:").nth(1)?.trim().parse().ok();
        }
    }
    None
}

/* // Function to extract the audio from the file
fn run_ffmpeg_extract_audio(file: &str) -> Result<(), io::Error> {
    // Run ffmpeg to extract the audio from the file
    let mut command = Command::new("ffmpeg")
        .arg("-i")
        .arg(file)
        .arg("-vn")
        .arg("-acodec")
        .arg("copy")
        .arg("-y")
        .arg("-f")
        //
} */

fn get_fps(file: &str) -> String {
    // Get the fps of the input file
    let fps = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=r_frame_rate")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");

    let raw_framerate = String::from_utf8(fps.stdout).unwrap().trim().to_string();
    let split_framerate = raw_framerate.split("/");
    let vec_framerate: Vec<&str> = split_framerate.collect();
    let frames: f32 = vec_framerate[0].parse().unwrap();
    let seconds: f32 = vec_framerate[1].parse().unwrap();
    return (frames / seconds).to_string();
}

pub fn get_fps_f32(file: &str) -> f32 {
    // Get the fps of the input file
    let fps = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=r_frame_rate")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");

    let raw_framerate = String::from_utf8(fps.stdout).unwrap().trim().to_string();
    let split_framerate = raw_framerate.split("/");
    let vec_framerate: Vec<&str> = split_framerate.collect();
    let frames: f32 = vec_framerate[0].parse().unwrap();
    let seconds: f32 = vec_framerate[1].parse().unwrap();
    return frames / seconds;
}

pub fn get_bitrate(file: &str) -> String {
    // Get the bitrate of the input file
    let bitrate = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=bit_rate")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");

    let raw_bitrate = String::from_utf8(bitrate.stdout)
        .unwrap()
        .trim()
        .to_string();
    if raw_bitrate.contains("N/A") {
        let bitrate = Command::new("ffprobe")
            .arg("-i")
            .arg(file)
            .arg("-v")
            .arg("error")
            .arg("-select_streams")
            .arg("v")
            .arg("-show_entries")
            .arg("format=bit_rate")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .output()
            .expect("failed to execute process");

        let raw_bitrate = String::from_utf8(bitrate.stdout)
            .unwrap()
            .trim()
            .to_string();
        let bitrate = raw_bitrate.parse::<f32>().unwrap() / 1000.0;
        return bitrate.to_string();
    } else {
        let bitrate = raw_bitrate.parse::<f32>().unwrap() / 1000.0;
        return bitrate.to_string();
    }
}

pub fn absolute_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();

    let absolute_path = (if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .expect("could not get current path")
            .join(path)
    })
    .clean();

    absolute_path.into_os_string().into_string().unwrap()
}

pub fn walk_count(dir: &String) -> usize {
    let scan_bar = ProgressBar::new_spinner();
    let scan_style =
        "[scan][{elapsed_precise}][{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} [scanned files] eta: {eta:<7}";
    scan_bar.set_style(
        ProgressStyle::default_spinner()
            .template(scan_style)
            .unwrap(),
    );

    let dir_files: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().unwrap().is_file())
        .collect();

    let dir_files_count = dir_files.len();
    scan_bar.set_length(dir_files_count as u64);

    let count = AtomicUsize::new(0);

    dir_files.into_par_iter().for_each(|e| {
        let mime = find_mimetype(e.path());
        if mime == "VIDEO" {
            scan_bar.inc(1);
            count.fetch_add(1, Ordering::Relaxed);
        }
    });

    let count = count.load(Ordering::Relaxed);

    println!("Found {} valid video files in folder!", count);
    count
}

pub fn walk_files(dir: &String) -> Vec<String> {
    let mut arr = vec![];
    let mut index = 0;

    for e in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if e.metadata().unwrap().is_file() {
            let mime = find_mimetype(e.path());
            if mime.to_string() == "VIDEO" {
                //println!("{}", e.path().display());
                arr.insert(index, e.path().display().to_string());
                index = index + 1;
            }
        }
    }
    return arr;
}

fn find_mimetype(filename: &Path) -> &'static str {
    let mut mime_types = HashMap::new();
    mime_types.insert("mkv", "VIDEO");
    mime_types.insert("avi", "VIDEO");
    mime_types.insert("mp4", "VIDEO");
    mime_types.insert("divx", "VIDEO");
    mime_types.insert("flv", "VIDEO");
    mime_types.insert("m4v", "VIDEO");
    mime_types.insert("mov", "VIDEO");
    mime_types.insert("ogv", "VIDEO");
    mime_types.insert("ts", "VIDEO");
    mime_types.insert("webm", "VIDEO");
    mime_types.insert("wmv", "VIDEO");

    let extension = filename
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    mime_types.get(extension).unwrap_or(&"OTHER")
}

fn possible_mode_values(s: &str) -> Result<String, String> {
    // ["default", "chunked"]
    let possible_values = vec!["default", "chunked"];
    if possible_values.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(String::from_str("invalid mode").unwrap())
    }
}

fn possible_pool_values(s: &str) -> Result<String, String> {
    // ["min", "harmonic_mean", "mean"}
    let possible_values = vec!["min", "harmonic_mean", "mean"];
    if possible_values.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(String::from_str("invalid pool").unwrap())
    }
}

// validate thread input, must be integer, and not exceed the number of logical cores
fn vmaf_thread_input_validation(s: &str) -> Result<String, String> {
    let re = Regex::new(r"^[0-9]+$").unwrap();
    if !re.is_match(s) {
        return Err(String::from_str("input must be an integer").unwrap());
    }
    let s = s.parse::<i32>().unwrap();
    if s > (num_cpus::get() as i32) {
        return Err(String::from_str("input must not exceed the number of logical cores").unwrap());
    }
    Ok(s.to_string())
}

fn vmaf_subsample_input_validation(s: &str) -> Result<String, String> {
    let re = Regex::new(r"^[0-9]+$").unwrap();
    if !re.is_match(s) {
        return Err(String::from_str("input must be an integer").unwrap());
    }
    let s = s.parse::<i32>().unwrap();
    if s > 100 {
        return Err(String::from_str("input must not exceed 100").unwrap());
    }
    Ok(s.to_string())
}

fn input_validation(s: &str) -> Result<String, String> {
    let p = Path::new(s);

    // if the path in p contains a double quote, remove it and everything after it
    if p.to_str().unwrap().contains("\"") {
        let mut s = p.to_str().unwrap().to_string();
        s.truncate(s.find("\"").unwrap());
        return Ok(s);
    }

    if p.is_dir() {
        return Ok(String::from_str(s).unwrap());
    }

    if !p.exists() {
        return Err(String::from_str("input path not found").unwrap());
    }

    match p.extension().unwrap().to_str().unwrap() {
        "mp4" | "mkv" | "avi" => Ok(s.to_string()),
        _ => Err(String::from_str("valid input formats: mp4/mkv/avi").unwrap()),
    }
}

struct StringBuilder {
    buffer: String,
}

impl StringBuilder {
    fn new() -> StringBuilder {
        StringBuilder {
            buffer: String::new(),
        }
    }

    fn push_str(&mut self, s: &str) {
        self.buffer.push_str(s);
    }

    fn push(&mut self, c: char) {
        self.buffer.push(c);
    }

    fn to_string(&self) -> String {
        self.buffer.clone()
    }
}

pub fn run_ffmpeg_scene_change(file: &str, args: &Args) -> Result<Vec<f32>, io::Error> {
    // Get the file's duration from ffprobe
    let duration = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");

    let duration_str = String::from_utf8(duration.stdout)
        .unwrap()
        .trim()
        .to_string();

    let duration_f32 = duration_str.parse::<f32>().unwrap();

    // Get the total duration
    let total_duration = duration_f32;

    // Create a progress bar
    let progress_bar = ProgressBar::new(total_duration as u64);
    let progress_bar_style =
        "[scd][{elapsed_precise}][{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]";
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template(progress_bar_style)
            .unwrap(),
    );

    // Run ffmpeg to detect scene changes
    let mut command = Command::new("ffmpeg")
        .arg("-i")
        .arg(file)
        .arg("-vf")
        .arg("select='gt(scene,0.4)',showinfo")
        .arg("-f")
        .arg("NULL")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute process");

    // list of scene changes
    let mut scene_changes_list: Vec<f32> = Vec::new();
    let mut scene_count = 0;

    // Add 00:00:00.000 to the beginning of the scene_changes_list
    scene_changes_list.push(0.0);

    loop {
        let mut buffer = [0; 1024];
        match command.stderr.as_mut().unwrap().read(&mut buffer) {
            Ok(n) => {
                // if line contains 'pts_time:' then print it
                let line = String::from_utf8_lossy(&buffer[..n]);
                if line.contains("pts_time:") {
                    scene_count += 1;
                    // Update the progress bar based on the current pts_time
                    let current_pts_time = line.split("pts_time:").collect::<Vec<&str>>()[1]
                        .split(" ")
                        .collect::<Vec<&str>>()[0]
                        .parse::<f32>()
                        .unwrap();
                    // if difference between current pts_time and last pts_time is less than args.scene_split_min second, skip
                    if current_pts_time - scene_changes_list[scene_changes_list.len() - 1]
                        < args.scene_split_min
                    {
                        // add the current pts_time to the scene_changes_list
                        //scene_changes_list.push(current_pts_time);
                        continue;
                    } else {
                        // add the current pts_time to the scene_changes_list
                        scene_changes_list.push(current_pts_time);
                    }
                    // add the current pts_time to the scene_changes_list
                    //scene_changes_list.push(current_pts_time);
                    progress_bar.set_position(current_pts_time as u64);
                }
                // if line contains 'out#0' then break
                if line.contains("out#0") {
                    // set the progress bar to the total duration
                    progress_bar.set_position(total_duration as u64);
                    progress_bar.finish();
                    break;
                }
            }
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
        }
    }

    /*     for scene_change in scene_changes_list.clone() {
        println!("scene_change: {}", scene_change);
    } */

    // Finish the progress bar
    progress_bar.finish();

    println!("Number of detected scenes: {}", scene_count);

    // print the scene changes
    //println!("Scene changes: {:?}", scene_changes_list);

    // add the total duration to the end of the scene_changes_list
    scene_changes_list.push(total_duration);

    Ok(scene_changes_list)
}

pub fn run_ffmpeg_extract_scene_changes_pipe_vmaf_target_threaded(
    file: &str,
    scene_changes: &[f32],
    scene_sizes: &Vec<(i32, i32)>,
    args: &Args,
    fps: &f32,
) -> Result<Vec<(i32, f32, f32)>, io::Error> {
    let thread_count = args.vmaf_threads.parse::<usize>().unwrap_or_else(|_| 4); // Default to 4 if parsing fails
    let threadpool = ThreadPool::new(thread_count);
    let scene_changes_len = scene_changes.len();
    let scene_changes = Arc::new(Mutex::new(scene_changes.to_vec()));
    // Arc vector to store, index, original_size and encoded_size
    let chunk_sizes = Arc::new(Mutex::new(Vec::<(i32, f32, f32)>::new()));
    let scene_sizes_clone = Arc::new(Mutex::new(scene_sizes.clone()));
    let mut i = 0;
    let file_size = get_file_size(&file).unwrap_or_else(|_| 0.0);

    // Get the file name of the input file
    let file_name = Path::new(&file).file_name().unwrap().to_str().unwrap();
    // Get the part before the extension
    let file_name_ = file_name.split('.').next().unwrap();
    // Get the extension
    let file_extension = Path::new(&file).extension().unwrap().to_str().unwrap();
    // Get the part after the extension
    let file_extension_ = file_extension.split('.').next().unwrap();
    // Make the output_file name like file_name_ + . + encoder + . + subsample + . + vmaf_target + . + extension
    let output_filename = format!(
        "{}.{}.vmaf{}.{}.subsample{}.{}",
        file_name_, args.encoder, args.vmaf, args.vmaf_pool, args.vmaf_subsample, file_extension_
    );
    println!("Output file name: {}", output_filename);
    //exit(1);

    let mut scenes: Vec<(i32, f32, f32)> = Vec::new();
    {
        let scene_changes_locked = scene_changes.lock().unwrap();
        for window in scene_changes_locked.windows(2) {
            scenes.push((i, window[0], window[1]));
            i += 1;
        }
    }

    // Sort scenes by duration, from longest to shortest
    // The sorting should be based on the duration between the the first and the second value in the tuple
    scenes.sort_by(|a, b| (b.2 - b.1).partial_cmp(&(a.2 - a.1)).unwrap());

    // Sort scenes by duration, from shortest to longest
    scenes.reverse();

    // TODO: add an argument to control this

    println!(
        "{} scenes to process (due to minimum duration of {} seconds per scene)",
        scenes.len(),
        args.scene_split_min
    );

    let vmaf_scores = Arc::new(Mutex::new(Vec::<(i32, f32, f32)>::new()));
    let m = Arc::new(Mutex::new(MultiProgress::new()));

    // Get the number of frames in the file
    let total_frames = get_framecount_tag(&file).unwrap_or_else(|_| {
        get_framecount_metadata(&file).unwrap_or_else(|_| {
            get_framecount_ffmpeg(&file)
                .unwrap_or_else(|_| get_framecount(&file).unwrap_or_else(|_| 0.0))
        })
    }) as u64;

    // Create a progress bar
    let frames_bar = Arc::new(Mutex::new(ProgressBar::new(total_frames as u64)));
    let frames_bar_style = "[frames][{elapsed_precise}][{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]";
    frames_bar.lock().unwrap().set_style(
        ProgressStyle::default_bar()
            .template(frames_bar_style)
            .unwrap(),
    );

    // Create a progress bar
    let info_vmaf_bar = Arc::new(Mutex::new(ProgressBar::new(scene_changes_len as u64)));
    let info_vmaf_bar_style = "[info][{spinner}][{msg}]";
    info_vmaf_bar.lock().unwrap().set_style(
        ProgressStyle::default_bar()
            .template(info_vmaf_bar_style)
            .unwrap(),
    );

    // Add optimal_vmaf_bar to multi-progress bar as a child
    m.lock().unwrap().add(frames_bar.lock().unwrap().clone());
    m.lock().unwrap().add(info_vmaf_bar.lock().unwrap().clone());

    // Enable steady tick on the progress bars
    /*     frames_bar
        .lock()
        .unwrap()
        .enable_steady_tick(Duration::from_millis(1000));
    */

    //TODO: fix this
    //function to extract all audio, video and subs
    extract_non_video_content(file, "temp.mkv")?;

    // vector of index, start_frame, end_frame, total_frames
    let scenes_clone = scenes.clone();
    let mut scenes_frames = Vec::<(i32, f32, f32, f32)>::new();
    let mut scenes_frames_index = 0;
    let mut scenes_frames_nosum = Vec::<(i32, f32, f32, f32)>::new();

    for (_scene_index, scene_change, next_scene_change) in scenes_clone {
        let start_frame = scene_change * fps;
        let end_frame = next_scene_change * fps;
        let scene_frames = end_frame - start_frame;
        let scene_frames_rounded = scene_frames.round();

        // add scene_index to scenes_details
        scenes_frames.push((
            scenes_frames_index as i32,
            start_frame,
            end_frame,
            scene_frames_rounded,
        ));

        // add scene_index to scenes_frames_nosum
        scenes_frames_nosum.push((
            scenes_frames_index as i32,
            start_frame,
            end_frame,
            scene_frames,
        ));

        scenes_frames_index += 1;
    }

    // make scenes_frames that if entries are 0, 2, 5, 3, 4 then the result should be 0, 2, 7, 10, 14
    let scenes_frames = scenes_frames
        .iter()
        .scan(0.0, |sum, (index, start_frame, end_frame, scene_frames)| {
            let new_sum = *sum + scene_frames;
            Some((*index, *start_frame, *end_frame, new_sum))
        })
        .collect::<Vec<(i32, f32, f32, f32)>>();

    // make scenes_frames that if entries are 0, 2, 5, 3, 4 then the result should be 0, 2, 7, 10, 14
    let mut scenes_frames_nosum = scenes_frames_nosum
        .iter()
        .scan(0.0, |sum, (index, start_frame, end_frame, scene_frames)| {
            let new_sum = *sum + scene_frames;
            Some((*index, *start_frame, *end_frame, new_sum))
        })
        .collect::<Vec<(i32, f32, f32, f32)>>();

    // print all scenes_frames
    //println!("{:?}", scenes_frames);
    // print the sum of all scene_frames
    //println!("{}", scenes_frames.iter().map(|(_, _, _, scene_frames)| scene_frames).sum::<f32>());
    // print the number of scenes
    //println!("{}", scenes_frames.len());
    //exit(1);

    let i = Arc::new(AtomicUsize::new(1)); // Initialize atomic integer at 1

    // create a done.txt file if it doesn't exist
    if !Path::new("done.txt").exists() {
        fs::File::create("done.txt")?;
    } else {
        // for each line in done.txt, which represents the done scenes, remove the corresponding index from the scenes vector
        let total_scenes = scenes.len();
        let done = fs::read_to_string("done.txt")?;
        let done = done.lines();
        for line in done {
            let line = line.parse::<i32>().unwrap();
            scenes = scenes
                .into_iter()
                .filter(|(index, _, _)| *index != line)
                .collect::<Vec<(i32, f32, f32)>>();
            scenes_frames_nosum = scenes_frames_nosum
                .into_iter()
                .filter(|(index, _, _, _)| *index != line)
                .collect::<Vec<(i32, f32, f32, f32)>>();
        }

        // print the number of scenes left
        println!("{} scenes left", scenes.len());

        // print the number of scenes skipped
        println!("{} scenes skipped", total_scenes - scenes.len());
        //exit(1);

        // TEMP List all in scenes_frames_nosum
        //println!("{:?}", scenes_frames_nosum);

        // TEMP List all in scenes_frames
        //println!("{:?}", scenes_frames);
        //exit(1);

        // set frames_bar position to the total of frames of all the scenes in done.txt
        frames_bar.lock().unwrap().set_position(
            total_frames
                - scenes_frames_nosum
                    .iter()
                    .map(|(_, _, _, scenes_frames_nosum)| *scenes_frames_nosum as u64)
                    .sum::<u64>(),
        );

        // set i to total_scenes - scenes.len()
        //println!("{}", total_scenes - scenes.len());
        //exit(1);
        i.store(
            total_scenes - scenes.len(),
            std::sync::atomic::Ordering::SeqCst,
        );
        info_vmaf_bar.lock().unwrap().set_message(format!(
            "{}/{}",
            total_scenes - scenes.len(),
            total_scenes
        ));
    }

    info_vmaf_bar
        .lock()
        .unwrap()
        .enable_steady_tick(Duration::from_millis(100));

    for (index, scene_change, next_scene_change) in scenes {
        let file = file.to_string(); // Clone the file path for the thread
        let args = args.clone(); // Clone args for the thread

        let vmaf_scores_clone = Arc::clone(&vmaf_scores);
        let frames_bar_clone = Arc::clone(&frames_bar);
        let info_vmaf_bar_clone = Arc::clone(&info_vmaf_bar);
        let scene_frames_clone = scenes_frames.clone();
        let scene_frames_len = scene_frames_clone.clone().len();
        let i_clone = Arc::clone(&i); // Clone atomic integer
        let chunk_sizes_clone = Arc::clone(&chunk_sizes);
        let scene_sizes_clone_clone = scene_sizes_clone.clone();

        threadpool.execute(move || {
            let fps = get_fps(&file);
            let ss_arg = format_timecode(&scene_change);
            let to_arg = format_timecode(&next_scene_change);

            let scene_size = get_scene_size(&file, &ss_arg, &to_arg).unwrap();
            let mut encoded_size = 0;

            // Find the best CRF for the scene
            //if let Ok((crf, vmaf_score)) = process_scene_adjust_crf(
            if let Ok((crf, vmaf_score)) = process_scene_adjust_crf_binary(
                index,
                scene_size,
                &file,
                &ss_arg,
                &to_arg,
                &fps,
                args.vmaf as f32,
                &args,
                vmaf_scores_clone.clone(),
                &args.vmaf_pool,
                &args.vmaf_threads,
                &args.vmaf_subsample,
            ) {
                // Encode the scene
                let encode_result = process_video_scene_encoded(
                    &file,
                    &index,
                    &args,
                    &crf,
                    &ss_arg,
                    &to_arg,
                    &frames_bar_clone.clone(),
                    &scene_frames_clone,
                );

                if encode_result.is_err() {
                    println!("Failed to encode scene: {}", encode_result.unwrap_err());
                } else {
                    match encode_result {
                        Ok((_, value)) => encoded_size = value,
                        Err(e) => println!("An error occurred: {:?}", e),
                    }
                }

                // TEMP show the estimated output size, based on already encoded files, and percentage of the total reduction in size
                //println!("scene size: {}, encoded size: {}", scene_size, encoded_size);
                //let reduction = ((scene_size - encoded_size) as f64 / scene_size as f64) * 100.0;
                //println!("{:.2}% reduction", reduction);

                // print all in chunk_sizes
                //println!("{:?}", chunk_sizes_clone.lock().unwrap());
                // print all in scene_sizes
                //println!("{:?}", scene_sizes_clone_clone.lock().unwrap());
                //exit(1);

                chunk_sizes_clone.lock().unwrap().push((
                    index,
                    scene_size as f32,
                    encoded_size as f32,
                ));
                // create a chunks.txt file if it doesn't exist
                if !Path::new("chunks.txt").exists() {
                    fs::File::create("chunks.txt").unwrap();
                } else {
                    // append the scene index to done.txt
                    let mut file = fs::OpenOptions::new()
                        .append(true)
                        .open("chunks.txt")
                        .unwrap();
                    writeln!(
                        file,
                        "index: {}, scene_size: {}, encoded_size: {}",
                        index, scene_size, encoded_size
                    )
                    .unwrap();
                }

                // get the size based of all already encoded files in the same folder as the input file, convert it to MB
                // They are named scene_xxx_encoded.mkv
                let mut already_encoded_size = 0.0;
                let mut original_size = 0.0;
                for entry in fs::read_dir(".").unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if path.is_file() {
                        let file_name = path.file_name().unwrap().to_str().unwrap();
                        if file_name.starts_with(&format!("scene_"))
                            && file_name.ends_with("_encoded.mkv")
                        {
                            already_encoded_size +=
                                fs::metadata(file_name).unwrap().len() as f32 / 1024.0 / 1024.0;
                            //println!("{} size: {:.2} MB", file_name, size);
                            // get the index of the scene filename
                            let index = file_name
                                .replace("scene_", "")
                                .replace("_encoded.mkv", "")
                                .parse::<i32>()
                                .unwrap();

                            //println!("index: {}", index);
                            //let scene_encoded_size = chunk_sizes_clone.lock().unwrap()[index as usize].1;
                            let scene_encoded_size = scene_sizes_clone_clone
                                .lock()
                                .unwrap()
                                .iter()
                                .find(|(idx, _)| *idx == index)
                                .map(|(_, size)| *size)
                                .unwrap()
                                as f32
                                / 1024.0;
                            //println!("Scene {} encoded size: {} kB", index, scene_encoded_size);
                            original_size += scene_encoded_size;
                        }
                    }
                }

                // get the size of temp.mkv in the same folder as the input file
                let temp_size = fs::metadata("temp.mkv").unwrap().len() as f32 / 1024.0 / 1024.0;
                //println!("temp.mkv size: {:.2} MB", temp_size);
                let estimated_output_size = chunk_sizes_clone
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(_, _, encoded_size)| *encoded_size)
                    .sum::<f32>()
                    / 1024.0
                    / 1024.0;
                //println!("estimated output size: {:.2} MB", estimated_output_size);
                let final_estimate = temp_size + already_encoded_size + estimated_output_size;
                let final_original = temp_size + (original_size);

                // Calculate reduction
                let total_reduction =
                    ((final_original - final_estimate) as f64 / final_original as f64) * 100.0;
                //println!("Total reduction: {:.2}%", total_reduction);

                // Calculate estimated output size, based on current total reduction
                let final_file_size = (file_size / 1024.0 / 1024.0) - temp_size;
                let estimated_output_size =
                    temp_size + (final_file_size * (100.0 - total_reduction as f32) / 100.0);
                //println!("estimated output size: {:.2} MB", estimated_output_size);

                // append the scene index to done.txt
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .open("done.txt")
                    .unwrap();
                writeln!(file, "{}", index).unwrap();

                // Update the progress bar
                let current_i = i_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst); // Increment atomic integer
                update_info_vmaf_bar(
                    info_vmaf_bar_clone.clone(),
                    total_reduction,
                    estimated_output_size,
                    current_i,
                    scene_frames_len,
                    index,
                    vmaf_score,
                    crf,
                );
            }
        });
    }

    threadpool.join();

    info_vmaf_bar.lock().unwrap().finish();

    // Merge all file named scene_{}_*.mkv, where {} is the scene index, and * is anything, into one, by order of scene index
    // The files are in the same folder as the input file
    // Use ffmpeg to concatenate the files
    let concatenante_result = concatenate_videos(&output_filename);

    // delete the done.txt file if the concatenation was successful
    if !concatenante_result.is_err() {
        fs::remove_file("done.txt").unwrap();
    } else {
        println!(
            "Failed to concatenate videos: {}",
            concatenante_result.unwrap_err()
        );
    }

    // Print average VMAF score and lowest VMAF score
    //println!("Average VMAF score: {}", vmaf_scores.lock().unwrap().iter().sum::<f32>() / vmaf_scores.lock().unwrap().len() as f32);
    //println!("Lowest VMAF score: {}", vmaf_scores.lock().unwrap().iter().min().unwrap());

    let final_scores = vmaf_scores.lock().unwrap().clone();
    Ok(final_scores)
}

fn update_info_vmaf_bar(
    info_vmaf_bar: Arc<Mutex<ProgressBar>>,
    total_reduction: f64,
    estimated_output_size: f32,
    i: usize,
    scene_changes_len: usize,
    scene_index: i32,
    vmaf_score: f32,
    crf: f32,
) {
    // Reverse the percentage calculation
    let total_reduction = total_reduction * -1.0;
    let reduction_message = if total_reduction >= 0.0 {
        // Assuming total_reduction is positive or zero, format with green color
        format!("\x1b[31m+{:.2}%\x1b[0m", total_reduction)
    } else {
        // If total_reduction is negative, format with red color
        // replace "-" with "+"
        format!("\x1b[32m{:.2}%\x1b[0m", total_reduction)
    };

    let bar = info_vmaf_bar.lock().unwrap();
    let bar_message = format!(
        "{}/{}][{:.2}MB({})][Scene: {} VMAF: {} CRF: {}",
        i,
        scene_changes_len,
        estimated_output_size,
        reduction_message, // Use the prepared message here
        scene_index,
        vmaf_score,
        crf
    );

    bar.set_message(bar_message);
}

/// Adjusts the CRF value for a scene to achieve a target VMAF score with minimal iterations.
///
/// Arguments:
/// * `scene_index`: Index of the scene being processed.
/// * `file`: Path to the video file.
/// * `ss_arg`: Start time for the scene.
/// * `to_arg`: End time for the scene.
/// * `fps`: Frames per second of the video.
/// * `vmaf_target`: Target VMAF score to achieve.
/// * `args`: Program arguments.
/// * `vmaf_scores_clone`: Shared vector to store VMAF scores for each scene.
///
/// Returns:
/// A tuple containing the adjusted CRF value and the achieved VMAF score for the scene.
fn process_scene_adjust_crf_binary(
    scene_index: i32,
    scene_size: i32,
    file: &str,
    ss_arg: &str,
    to_arg: &str,
    fps: &str,
    vmaf_target: f32,
    args: &Args,
    vmaf_scores_clone: Arc<Mutex<Vec<(i32, f32, f32)>>>,
    vmaf_pool: &str,
    vmaf_threads: &str,
    vmaf_subsample: &str,
) -> Result<(f32, f32), String> {
    let mut crf = 23.0; // Starting CRF value, aiming for a 'middle ground'
    let mut min_crf = 10.0;
    let mut max_crf = 45.0;
    let mut best_vmaf = 0.0;
    let mut best_crf = crf;
    let max_iterations = 3;
    let mut iteration = 1;

    /*     // Check if vmaf_scores_clone already contains the scene index with the same CRF, if so, return
    if let Some((_scene_index, _crf, _vmaf_score)) = vmaf_scores_clone
        .lock()
        .unwrap()
        .iter()
        .find(|(_scene_index, _crf, _vmaf_score)| *_scene_index == scene_index)
    {
        // Write in debug.txt the CRF and VMAF score for the scene and that it was skipped because it was already processed
        let mut debug_file = fs::OpenOptions::new()
            .append(true)
            .open("debug.txt")
            .unwrap();
        writeln!(
            debug_file,
            "Scene: {}, CRF: {}, VMAF: {}, Iterations: {}, Scene Size: {}kB",
            scene_index, _crf, _vmaf_score, iteration, scene_size
        )
        .unwrap();
        return Ok((*_crf, *_vmaf_score));
    } */

    while iteration <= max_iterations {
        let vmaf = process_video_pipe_and_vmaf(
            &file.to_string(),
            args,
            &crf,
            &fps.to_string(),
            &ss_arg.to_string(),
            &to_arg.to_string(),
            &vmaf_pool.to_string(),
            &vmaf_threads.to_string(),
            &vmaf_subsample.to_string(),
        )
        .unwrap();

        let vmaf_score = parse_vmaf_score(&vmaf).unwrap_or(0.0);

        /*         let mut vmaf_size = helper::parse_size_output(&vmaf).unwrap();
        let pattern = Regex::new(r"\b\d+kB\b").unwrap();
        if let Some(matched) = pattern.find(&vmaf_size) {
            //println!("Matched size: {}", &vmaf_size[matched.start()..matched.end()]);
            // get only the numbers from the matched size string
            let numbers = &vmaf_size[matched.start()..matched.end()].replace("kB", "");
            vmaf_size = numbers.trim().parse::<f32>().unwrap().to_string();
            //println!("VMAF size in kB: {}", vmaf_size_final);
            //println!("Original size in kB: {}", scene_size);
        } else {
            //println!("No match found");
        } */

        //TEMP Append index, crf and vmaf_score to a file named debug.txt if exists, else create it
        if fs::metadata("debug.txt").is_err() {
            // If the file does not exist, create it
            fs::File::create("debug.txt").unwrap();
        }
        let mut debug_file = fs::OpenOptions::new()
            .append(true)
            .open("debug.txt")
            .unwrap();
        // TEMP get framerate and total frames of the current scene
        writeln!(
            debug_file,
            "Scene: {}, CRF: {}, VMAF: {}, Iterations: {}, Scene Size: {}kB",
            scene_index, crf, vmaf_score, iteration, scene_size
        )
        .unwrap();

        {
            let mut scores = vmaf_scores_clone.lock().unwrap();
            scores.push((scene_index, crf, vmaf_score));
        }

        if (vmaf_target - vmaf_score).abs() <= 0.5 {
            best_crf = crf;
            best_vmaf = vmaf_score;
            break; // Close enough to target VMAF, exit early
                   // if the vmaf is within 2.5 of the target, adjust crf only by 3.0
        } else if (vmaf_target - vmaf_score).abs() <= 3.0 && (vmaf_target - vmaf_score).abs() > 2.0
        {
            if vmaf_score > vmaf_target {
                crf += 3.0; // Increase CRF by 3.0 for lower quality
            } else {
                crf -= 3.0; // Decrease CRF by 3.0 for higher quality
            }
        // if the vmaf is within 1.5 of the target, adjust crf only by 2.0
        } else if (vmaf_target - vmaf_score).abs() <= 2.0 && (vmaf_target - vmaf_score).abs() > 1.0
        {
            if vmaf_score > vmaf_target {
                crf += 2.0; // Increase CRF by 2.0 for lower quality
            } else {
                crf -= 2.0; // Decrease CRF by 2.0 for higher quality
            }
        // if the vmaf is within 0.5 of the target, adjust crf only by 1.0
        } else if (vmaf_target - vmaf_score).abs() <= 1.0 && (vmaf_target - vmaf_score).abs() > 0.8
        {
            if vmaf_score > vmaf_target {
                crf += 1.0; // Increase CRF by 1.0 for lower quality
            } else {
                crf -= 1.0; // Decrease CRF by 1.0 for higher quality
            }
        // if the vmaf is within 0.8 of the target, adjust crf only by 0.5
        } else if (vmaf_target - vmaf_score).abs() <= 0.8 && (vmaf_target - vmaf_score).abs() > 0.5
        {
            if vmaf_score > vmaf_target {
                crf += 0.5; // Increase CRF by 0.5 for lower quality
            } else {
                crf -= 0.5; // Decrease CRF by 0.5 for higher quality
            }
        } else {
            if vmaf_score > vmaf_target {
                min_crf = crf - 1.0; // Need higher quality
            } else {
                max_crf = crf + 1.0; // Can afford lower quality
            }
            crf = min_crf + (max_crf - min_crf) / 2.0; // Update CRF to mid-point of new range
        }

        // Check if we have narrowed the range completely
        if min_crf > max_crf {
            break; // Exit if the search range is invalid
        }

        /*         if vmaf_score > vmaf_target {
            // High quality, decrease CRF (increase compression)
            crf = std::cmp::max(min_crf, crf - step_size);
        } else {
            // Low quality, increase CRF (decrease compression)
            crf = std::cmp::min(max_crf, crf + step_size);
        } */

        // Update best estimates if closer to the target VMAF score
        if (vmaf_target - vmaf_score).abs() < (vmaf_target - best_vmaf).abs() {
            best_crf = crf;
            best_vmaf = vmaf_score;
        }

        iteration += 1;
    }

    if best_vmaf != 0.0 {
        Ok((best_crf, best_vmaf))
    } else {
        Err("Failed to adjust CRF to target VMAF score within max iterations".to_string())
    }
}

fn extract_non_video_content(
    input_file: &str,
    output_file: &str,
) -> Result<Output, std::io::Error> {
    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(input_file)
        .arg("-vn") // Disable video
        .arg("-acodec")
        .arg("copy") // Copy audio streams without re-encoding
        .arg("-scodec")
        .arg("copy") // Copy subtitle streams without re-encoding
        .arg(output_file)
        .stdout(Stdio::piped()) // Capture stdout
        .stderr(Stdio::piped()) // Capture stderr to check for errors
        .output()?;

    Ok(output)
}

fn concatenate_videos(output_filename: &str) -> Result<(), std::io::Error> {
    // Step 1: Create an array of all filenames named scene_{}_*.mkv in current directory with walkdir
    let list_file_name = "list.txt";
    let mut list_file = File::create(list_file_name)?;
    for entry in WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().unwrap().starts_with("scene_"))
    {
        list_file.write_all(format!("file '{}'\n", entry.path().to_str().unwrap()).as_bytes())?;
    }

    // Step 2: Run FFmpeg to concatenate videos
    let ffmpeg_output = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(list_file_name)
        .arg("-c")
        .arg("copy")
        .arg("merged_scenes.mkv")
        .output()?;

    // Optional: Check FFmpeg command output for success or error
    if !ffmpeg_output.status.success() {
        let error_message = String::from_utf8_lossy(&ffmpeg_output.stderr);
        eprintln!("FFmpeg error: {}", error_message);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "FFmpeg failed to concatenate videos.",
        ));
    }

    // Step 3: Remove list file
    std::fs::remove_file(list_file_name)?;

    // Step 4: Merge merged_scenes.mkv with temp.mkv containing audio and subtitles
    let output = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg("temp.mkv")
        .arg("-i")
        .arg("merged_scenes.mkv")
        .arg("-c")
        .arg("copy")
        .arg(output_filename)
        .output()?;

    // Optional: Check FFmpeg command output for success or error
    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr);
        eprintln!("FFmpeg error: {}", error_message);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "FFmpeg failed to merge videos.",
        ));
    }

    println!("Videos concatenated successfully.");

    // Step 5: Delete merged_scenes.mkv
    std::fs::remove_file("merged_scenes.mkv")?;

    // Step 6: Delete temp.mkv
    std::fs::remove_file("temp.mkv")?;

    // Step 7: Delete all scene_{}_*.mkv files
    for entry in WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().unwrap().starts_with("scene_"))
    {
        std::fs::remove_file(entry.path())?;
    }

    Ok(())
}

// Function to encode a scene with a given CRF. The output file should be like: scene_{scene}_{crf}_encoded.mkv
fn process_video_scene_encoded(
    file: &String,
    scene_index: &i32,
    args: &Args,
    crf: &f32,
    ss_arg: &String,
    to_arg: &String,
    frames_bar: &Arc<Mutex<ProgressBar>>,
    scene_frames: &Vec<(i32, f32, f32, f32)>,
) -> Result<(Output, i32), io::Error> {
    // set preset to the preset linked to encoder
    let preset = match args.encoder.as_str() {
        "libx265" => &args.preset_x265,
        "hevc_nvenc" => &args.preset_hevc_nvenc,
        "hevc_qsv" => &args.preset_hevc_qsv,
        "av1" => &args.preset_libaom_av1,
        "av1_qsv" => &args.preset_av1_qsv,
        "libsvtav1" => &args.preset_libsvtav1,
        _ => &args.preset_x265,
    };

    let params = match args.encoder.as_str() {
        "libx265" => &args.params_x265,
        "hevc_nvenc" => &args.params_hevc_nvenc,
        "hevc_qsv" => &args.params_hevc_qsv,
        "av1" => &args.params_libaom_av1,
        "av1_qsv" => &args.params_av1_qsv,
        "libsvtav1" => &args.params_libsvtav1,
        _ => &args.params_x265,
    };

    // Prefix the scene_index with numbers that can be sorted
    let scene_index = format!("{:03}", scene_index);

    let output_file = format!("scene_{}_encoded.mkv", scene_index);

    let return_size = Arc::new(AtomicI32::new(0));

    let mut command = Command::new("./ffmpeg.exe");
    command
        .arg("-hide_banner")
        .arg("-y")
        //.arg("-r")
        //.arg(format!("{}", helper::get_fps_f32(file)))
        .arg("-i")
        .arg(file)
        .arg("-map_metadata")
        .arg("-1")
        .arg("-ss")
        .arg(&ss_arg)
        .arg("-to")
        .arg(&to_arg)
        .arg("-c:v")
        .arg(&args.encoder)
        .arg("-preset")
        .arg(&preset);
    // for each parameter in params separated by space add it
    for param in params.split(' ') {
        command.arg(param);
    }
    // TEMP to improve
    command.arg("-g");
    command.arg(format!("{}", get_fps_f32(file) * 10.0));

    if args.encoder == "hevc_nvenc" {
        command
            .arg("-rc:v")
            .arg("vbr")
            .arg("-cq:v")
            .arg(crf.to_string())
            .arg("-qmin")
            .arg(crf.to_string())
            .arg("-qmax")
            .arg(crf.to_string());
        //.arg(params.to_string());
    } else if args.encoder == "hevc_qsv" {
        command.arg("-global_quality").arg(crf.to_string());
        //.arg(&args.params_hevc_qsv);
    } else if args.encoder == "av1_qsv" {
        command.arg("-global_quality").arg(crf.to_string());
        //.arg(&args.params_av1_qsv);
    } else if args.encoder == "libx265" {
        command.arg("-crf").arg(crf.to_string());
        //.arg(&args.params_x265);
    } else if args.encoder == "av1" {
    } else if args.encoder == "libsvtav1" {
    }
    command
        .arg("-pix_fmt")
        .arg("yuv420p10le")
        .arg("-an")
        .arg("-sn")
        .arg("-dn")
        .arg("-vf")
        .arg("showinfo")
        .arg(output_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // TEMP Print the command without quotes, except for the input file
    //println!("command: {:?}", command);
    //exit(1);

    let mut spawned_command = command.spawn()?;
    let stderr = spawned_command
        .stderr
        .take()
        .expect("Failed to capture stderr");

    // Spawn a thread to read the stderr output
    let frames_bar_clone = frames_bar.clone();

    let scene_index_i32 = scene_index
        .parse::<i32>()
        .expect("Failed to parse scene_index as i32");
    //get the number of frames for this scene
    let scene_frames = scene_frames
        .iter()
        .filter(|frame| frame.0 == scene_index_i32)
        .collect::<Vec<_>>();

    let scene_frames_count = scene_frames.iter().map(|frame| frame.3).sum::<f32>();
    let mut scene_frames_status = 0;
    // TEMP Print the scene_frames
    //println!("{:?}", scene_frames.iter().map(|frame| frame.3).sum::<f32>());

    let return_size_clone = return_size.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        reader
            .lines()
            .filter_map(|line| line.ok())
            .for_each(|line| {
                // Parse the stderr output for frame information
                //TEMP Show the stderr output
                //println!("stderr: {}", line);

                // try to set the progress bar to the value parsed like: n:  59 pts:   2461
                // get the value like: n:  59

                if let Some(progress) = parse_frame_progress(&line) {
                    if scene_frames_status < scene_frames_count as usize {
                        let bar = frames_bar_clone.lock().unwrap();
                        bar.inc(progress); // Increment the progress bar
                        scene_frames_status += 1;
                    }
                }

                if let Some(mut size) = parse_encode_size_output(&line) {
                    let pattern = Regex::new(r"\b\d+kB\b").unwrap();
                    if let Some(matched) = pattern.find(&size) {
                        //println!("Matched size: {}", &vmaf_size[matched.start()..matched.end()]);
                        // get only the numbers from the matched size string
                        let numbers = &size[matched.start()..matched.end()].replace("kB", "");
                        size = numbers.trim().parse::<i32>().unwrap().to_string();
                        let numeric_size = size.parse::<i32>().unwrap();
                        // if return_size is greater than 0, set it
                        if numeric_size > 0 {
                            return_size_clone.store(
                                numbers.trim().parse::<i32>().unwrap(),
                                std::sync::atomic::Ordering::SeqCst,
                            );
                        }
                    } else {
                        //println!("No match found");
                    }

                    //TEMP Append index, crf and vmaf_score to a file named debug.txt if exists, else create it
                    if fs::metadata("debug.txt").is_err() {
                        // If the file does not exist, create it
                        fs::File::create("debug.txt").unwrap();
                    }
                    let mut debug_file = fs::OpenOptions::new()
                        .append(true)
                        .open("debug.txt")
                        .unwrap();
                    // TEMP get framerate and total frames of the current scene
                    writeln!(
                        debug_file,
                        "Scene: {}, Scene Size: {}kB",
                        scene_index, &size
                    )
                    .unwrap();
                }
            });
    });

    let return_size_clone = return_size.clone();
    let output = spawned_command.wait_with_output()?;

    // set progress bar to the 3rd value of scene_frames of the current scene index
    //let frames_bar = frames_bar.lock().unwrap();
    //let scene_index_usize = scene_index.parse::<usize>().unwrap(); // Convert scene_index to usize
    //frames_bar.set_position(scene_frames[scene_index_usize].2 as u64);

    // TODO Get the number of frames in the chunk
    //let mut frames_bar = frames_bar.lock().unwrap();
    //frames_bar.set_length(*scene_frames);

    let return_size = return_size_clone.load(std::sync::atomic::Ordering::SeqCst);

    return Ok((output, return_size));
}

// Implement parse_frame_progress to parse the ffmpeg stderr output
fn parse_frame_progress(line: &str) -> Option<u64> {
    // This function needs to parse lines from ffmpeg's stderr to find frame processing updates.
    // Adjust the parsing logic based on the actual output format of ffmpeg.
    if line.contains("n:") {
        return Some(1);
    } else {
        return None;
    }
}

pub fn run_ffmpeg_extract_scene_changes_pipe_vmaf_target(
    file: &str,
    scene_changes: &Vec<f32>,
    args: &Args,
) -> Result<Vec<f32>, io::Error> {
    // Create a progress bar
    let progress_bar = ProgressBar::new(scene_changes.len() as u64);
    let progress_bar_style =
        "[extract][{elapsed_precise}][{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]";
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template(progress_bar_style)
            .unwrap(),
    );

    // Run ffmpeg to extract each scene change into a separate file
    let scene_changes_list: Vec<f32> = Vec::new();
    let mut scene_count = 0;
    let vmaf_target = args.vmaf as f32;
    let mut crf = 25;
    let mut i = 0;

    for scene_change in scene_changes.clone() {
        // If scene_change is the last item in the scene_changes vector, break
        if i == scene_changes.len() - 1 {
            break;
        }

        // Get the fps of the input file
        let fps = get_fps(&file);

        // Write the command like this, where fps=23.98 is the fps of the input file
        // Should use get_fps function to get the fps of the input file

        let output_file = "./temp_output.nut";

        let mut first_command = Command::new("./ffmpeg.exe")
            .arg("-y")
            .arg("-ss")
            .arg(format_timecode(&scene_change))
            .arg("-to")
            .arg(format_timecode(&scene_changes[i + 1]))
            .arg("-i")
            .arg(file)
            .arg("-c:v")
            .arg(&args.encoder)
            .arg("-preset")
            // todo: add support for other encoders
            .arg(&args.preset_hevc_nvenc)
            .arg("-rc:v")
            .arg("vbr")
            .arg("-cq:v")
            .arg("25")
            .arg("-qmin")
            .arg("25")
            .arg("-qmax")
            .arg("25")
            .arg("-pix_fmt")
            .arg("yuv420p10le")
            .arg("-f")
            .arg("nut")
            .arg(output_file) // Write output to a temporary file
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start first command");

        // Wait for the first command to finish
        let _ = first_command
            .wait()
            .expect("Failed to wait on the first command");

        // Second FFmpeg command
        let second_command = Command::new("./ffmpeg.exe")
            .arg("-ss")
            .arg(format_timecode(&scene_change))
            .arg("-to")
            .arg(format_timecode(&scene_changes[i + 1]))
            .arg("-i")
            .arg(file) // Read input from the temporary file
            .arg("-f")
            .arg("nut")
            .arg("-thread_queue_size")
            .arg("4096")
            .arg("-i")
            .arg(output_file)
            .arg("-lavfi")
            .arg(
                format!(
                    "[0:v]setpts=PTS-STARTPTS,fps={}[reference];[1:v]setpts=PTS-STARTPTS,fps={}[distorted];[reference][distorted]libvmaf",
                    fps,
                    fps
                )
            )
            .arg("-f")
            .arg("null")
            .arg("-")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start second command");

        let output = second_command.wait_with_output().unwrap();
        let output_str_stderr = String::from_utf8_lossy(&output.stderr);
        let split_output_stderr: Vec<&str> = output_str_stderr.split("\n").collect();

        let mut vmaf_score: f32 = 0.0;
        for line in split_output_stderr.clone() {
            if line.contains("VMAF score:") {
                vmaf_score = line.split("VMAF score:").collect::<Vec<&str>>()[1]
                    .trim()
                    .parse()
                    .unwrap();
                // print the score and crf, which is after "VMAF score:", but make sure to remove the trailing whitespace
                // if score is less than vmax_target, then print in red, else print in green
                // also print the scene index
                if vmaf_score < vmaf_target {
                    println!(
                        "Scene index: {}, VMAF score: {}, crf: {}",
                        scene_count,
                        line.split("VMAF score: ").collect::<Vec<&str>>()[1]
                            .trim()
                            .red(),
                        crf.to_string().red()
                    );
                } else {
                    println!(
                        "Scene index: {}, VMAF score: {}, crf: {}",
                        scene_count,
                        line.split("VMAF score: ").collect::<Vec<&str>>()[1]
                            .trim()
                            .green(),
                        crf.to_string().green()
                    );
                }
                break;
            }
        }

        while vmaf_score < vmaf_target || vmaf_score > vmaf_target + 1.0 {
            if vmaf_score < vmaf_target {
                crf -= 1;
            } else if vmaf_score > vmaf_target + 1.0 {
                crf += 1;
            }

            let output_file = "./temp_output.nut";

            let mut first_command = Command::new("./ffmpeg.exe")
                .arg("-y")
                .arg("-ss")
                .arg(format_timecode(&scene_change))
                .arg("-to")
                .arg(format_timecode(&scene_changes[i + 1]))
                .arg("-i")
                .arg(file)
                .arg("-c:v")
                .arg(&args.encoder)
                .arg("-preset")
                // todo: add support for other encoders
                .arg(&args.preset_hevc_nvenc)
                .arg("-rc:v")
                .arg("vbr")
                .arg("-cq:v")
                .arg(&crf.to_string())
                .arg("-qmin")
                .arg(&crf.to_string())
                .arg("-qmax")
                .arg(&crf.to_string())
                .arg("-pix_fmt")
                .arg("yuv420p10le")
                .arg("-f")
                .arg("nut")
                .arg(output_file) // Write output to a temporary file
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to start first command");

            // Wait for the first command to finish
            let _ = first_command
                .wait()
                .expect("Failed to wait on the first command");

            // Second FFmpeg command
            let second_command = Command::new("./ffmpeg.exe")
                .arg("-ss")
                .arg(format_timecode(&scene_change))
                .arg("-to")
                .arg(format_timecode(&scene_changes[i + 1]))
                .arg("-i")
                .arg(file) // Read input from the temporary file
                .arg("-f")
                .arg("nut")
                .arg("-thread_queue_size")
                .arg("4096")
                .arg("-i")
                .arg(output_file)
                .arg("-lavfi")
                .arg(
                    format!(
                        "[0:v]setpts=PTS-STARTPTS,fps={}[reference];[1:v]setpts=PTS-STARTPTS,fps={}[distorted];[reference][distorted]libvmaf",
                        fps,
                        fps
                    )
                )
                .arg("-f")
                .arg("null")
                .arg("-")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to start second command");

            let output = second_command.wait_with_output().unwrap();
            let output_str_stderr = String::from_utf8_lossy(&output.stderr);
            let split_output_stderr: Vec<&str> = output_str_stderr.split("\n").collect();

            vmaf_score = 0.0;
            for line in split_output_stderr.clone() {
                if line.contains("VMAF score:") {
                    vmaf_score = line.split("VMAF score:").collect::<Vec<&str>>()[1]
                        .trim()
                        .parse()
                        .unwrap();
                    // print the score and crf, which is after "VMAF score:", but make sure to remove the trailing whitespace
                    // if score is less than vmax_target, then print in red, else print in green
                    // also print the scene index
                    if vmaf_score < vmaf_target {
                        println!(
                            "Scene index: {}, VMAF score: {}, crf: {}",
                            scene_count,
                            line.split("VMAF score: ").collect::<Vec<&str>>()[1]
                                .trim()
                                .red(),
                            crf.to_string().red()
                        );
                    } else {
                        println!(
                            "Scene index: {}, VMAF score: {}, crf: {}",
                            scene_count,
                            line.split("VMAF score: ").collect::<Vec<&str>>()[1]
                                .trim()
                                .green(),
                            crf.to_string().green()
                        );
                    }
                    break;
                }
            }
        }

        fs::remove_file(output_file).expect("Failed to remove temporary file");

        scene_count += 1;
        i = i + 1;
        progress_bar.set_position(scene_count as u64);
    }

    println!("Extracted {} scenes", scene_count);

    Ok(scene_changes_list)
}

fn execute_crf_search(
    file: &str,
    encoder: &str,
    vmaf: i32,
    max_crf: &str,
    sample_every: &str,
    pix_fmt: &str,
    preset_x265: &str,
    vmaf_threads: &str,
    verbose: bool,
) -> Result<(bool, String), io::Error> {
    // prefix vmaf_threads it with 'n_threads='
    let vmaf_threads = format!("n_threads={}", vmaf_threads);
    let mut output = Command::new("ab-av1.exe")
        .arg("crf-search")
        .arg("-i")
        .arg(file)
        .arg("--min-vmaf")
        .arg(vmaf.to_string())
        .arg("--max-crf")
        .arg(max_crf)
        .arg("--sample-every")
        .arg(sample_every)
        .arg("-e")
        .arg(encoder)
        .arg("--pix-format")
        .arg(pix_fmt)
        .arg("--preset")
        .arg(preset_x265)
        .arg("--vmaf")
        .arg(&vmaf_threads)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if verbose {
        // Show the stderr output from ab-av1.exe
        let stderr = output.stderr.take().unwrap();
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            println!("{}", line.unwrap());
        }
    }

    let output_result = output.wait_with_output()?;

    if output_result.status.success() {
        let output_str = String::from_utf8_lossy(&output_result.stdout);
        let split_output: Vec<&str> = output_str.split_whitespace().collect();
        // crf 21 VMAF 97.15 predicted video stream size 6.60 GiB (72%) taking 21 minutes
        // - crf 19 VMAF 97.24 (76%) (cache)
        // - crf 23 VMAF 96.79 (59%) (cache)
        //
        // Encode with: ab-av1 encode -e hevc_nvenc -i "file.extension" --crf 21 --preset p7 --pix-format yuv420p10le
        // The above lines are examples of the output from ab-av1.exe
        // Get the crf value from the output
        let crf = split_output[1].to_string();

        return Ok((true, crf.to_string()));
    } else {
        Ok((false, "".to_string()))
    }
}

pub fn run_ab_av1_crf_search(
    file: &str,
    encoder: &str,
    preset_x265: &str,
    pix_fmt: &str,
    mut vmaf: i32,
    max_crf: &str,
    sample_every: &str,
    vmaf_threads: &str,
    verbose: bool,
    task_id: &str,
    current_file_count: &u64,
    total_files: &i32,
) -> Result<(String, i32), Error> {
    let _web_task_id = task_id.to_string();

    *WEB_TASK_ID_STATIC.lock().unwrap() = _web_task_id.clone();
    *WEB_FPS_STATIC.lock().unwrap() = 0;
    *WEB_CURRENT_FRAME_STATIC.lock().unwrap() = 0;
    *WEB_TOTAL_FRAME_STATIC.lock().unwrap() = 0.0;
    *WEB_EXPECTED_SIZE_STATIC.lock().unwrap() = 0.0;
    *WEB_CURRENT_FILE_STATIC.lock().unwrap() = current_file_count.clone() as u64;
    *WEB_TOTAL_FILES_STATIC.lock().unwrap() = total_files.clone() as u64;
    *WEB_CURRENT_FILE_NAME_STATIC.lock().unwrap() =
        format!("Searching for best CRF for VMAF {}...", vmaf);

    loop {
        // print searching for best crf for vmaf <value> in yellow
        println!(
            "{}",
            format!("Searching for best CRF for VMAF {}...", vmaf).yellow()
        );
        let (success, crf) = execute_crf_search(
            file,
            encoder,
            vmaf,
            max_crf,
            sample_every,
            pix_fmt,
            preset_x265,
            vmaf_threads,
            verbose,
        )?;

        if success {
            // show the new vmaf value at the CRF
            println!(
                "{}",
                format!("Found CRF {} for VMAF {}!", crf, vmaf).green()
            );
            return Ok((crf, vmaf));
        } else {
            if vmaf == 0 {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Failed to find a suitable CRF",
                ));
            }
            vmaf -= 1;

            // show the new vmaf value
            println!("{}", format!("Retrying with VMAF of {}...", vmaf).yellow());
        }
    }
}

pub fn run_ffmpeg_transcode(
    file: &str,
    encoder: &str,
    params_x265: &str,
    preset_x265: &str,
    pix_fmt: &str,
    output_folder: &str,
    target_crf: &str,
    file_bar: &ProgressBar,
    transcode_bar: &ProgressBar,
    total_bar: &ProgressBar,
    info_bar: &ProgressBar,
    codec_bar: &ProgressBar,
    total_files: &i32,
    current_file_count: &u64,
    vector_files_to_process_frame_count: &Vec<(String, u64)>,
    final_vmaf: &i32,
    original_audio_codec: &str,
    transcode_info: &str,
    vec_audio_args: &Vec<(usize, String, String)>,
    vec_video_args: &Vec<(usize, String, String, String)>,
    task_id: &str,
) {
    let target_crf = target_crf.trim();
    let _final_audio_codec: String;
    let _final_video_codec: String;
    let mut _web_fps: u64 = 0;
    let mut _web_current_frame: u64 = 0;
    let mut _web_total_frame: f32 = 0.0;
    let mut _web_expected_size: f32 = 0.0;
    let mut _web_current_file: u64 = current_file_count.clone() as u64;
    let mut _web_total_files: u64 = total_files.clone() as u64;
    let mut _web_current_file_name = file.to_string();
    let mut _web_progess: Progress;

    // Prepare ffmpeg command
    let mut cmd = Command::new("ffmpeg.exe");

    cmd.arg("-y").arg("-i").arg(file).arg("-c:v:0").arg(encoder);

    // add params_x265 to ffmpeg command
    for arg in params_x265.split_whitespace() {
        cmd.arg(arg);
    }

    // map video stream with -map 0:v:0
    cmd.arg("-map").arg("0:v:0");

    // count how many audio streams there are
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let temp_output = output.clone();
    let audio_stream = String::from_utf8(temp_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    let audio_streams_count = audio_stream.lines().count() as i32;

    // count how many subtitle streams there are
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("s")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let temp_output = output.clone();
    let subtitle_stream = String::from_utf8(temp_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    let subtitle_streams_count = subtitle_stream.lines().count() as i32;

    // map all audio streams with -map 0:a copy, loop over with audio_streams_count
    for i in 0..audio_streams_count {
        cmd.arg("-map").arg(format!("0:a:{}", i));
        cmd.arg(format!("-c:a:{}", i)).arg("copy"); // Corrected syntax here
    }

    // map all subtitle streams with -map 0:a copy, loop over with subtitle_streams_count
    for i in 0..subtitle_streams_count {
        cmd.arg("-map").arg(format!("0:s:{}", i));
        cmd.arg(format!("-c:s:{}", i)).arg("copy"); // Corrected syntax here
    }

    let metadata = run_ffmpeg_map_metadata(file);

    if metadata != "" {
        cmd.arg("-map_metadata").arg("-1");
        for arg in metadata.split_whitespace() {
            cmd.arg(arg);
        }
    } else {
        cmd.arg("-map_metadata").arg("0");
    }

    let mut final_audio_codec = String::new(); // Initialize with an empty string
    if !vec_audio_args.is_empty() {
        for (i, arg, codec) in vec_audio_args {
            if !arg.is_empty() {
                for arg in arg.split_whitespace() {
                    cmd.arg(arg);
                }
                if codec != "opus" || codec != "aac" || codec != "mp3" {
                    final_audio_codec.push_str(&format!("{}->{},", codec, "opus"));
                } else {
                    final_audio_codec.push_str(&format!("{},", codec));
                }
            } else if arg == "" {
                cmd.arg(format!("-c:a:{}", i)).arg("copy");
                if codec != "opus" || codec != "aac" || codec != "mp3" {
                    final_audio_codec.push_str(&format!("{}->{},", codec, "opus"));
                } else {
                    final_audio_codec.push_str(&format!("{},", codec));
                }
            }
        }
        final_audio_codec.pop(); // Remove the trailing comma
    } else {
        final_audio_codec = format!("{}->{}", original_audio_codec, "opus");
    }

    // Add video codec to final_video_codec string
    let mut final_video_codec = String::new(); // Initialize with an empty string
    if !vec_video_args.is_empty() {
        for (_i, codec, _width, _height) in vec_video_args {
            if !codec.is_empty() {
                final_video_codec.push_str(&format!("{}->{},", codec, "hevc"));
            } else if codec == "" {
                final_video_codec.push_str(&format!("{},", "hevc"));
            }
        }
        final_video_codec.pop(); // Remove the trailing comma
    } else {
        final_video_codec = "copy".to_string();
    }

    cmd.arg("-preset").arg(preset_x265);

    if encoder == "hevc_nvenc" {
        cmd.arg("-cq").arg(target_crf);
    }
    if encoder == "hevc_qsv" {
        // Get the fps from the input file and convert it to an integer value and multiply it by 2
        let fps = get_fps(&file).parse().unwrap_or_else(|_| 0.0);
        //cmd.arg("-g").arg(format!("{}", fps * 2.0));
        cmd.arg("-g").arg(format!("{}", fps * 10.0));
        // TEMP
        //cmd.arg("-bf").arg("2");
        cmd.arg("-global_quality:v").arg(target_crf);
    }

    cmd.arg("-pix_fmt")
        .arg(pix_fmt)
        .arg(Path::new(&output_folder));

    // Execute ffmpeg command
    let mut output = cmd
        .stdout(Stdio::piped()) // Redirect standard output
        .stderr(Stdio::piped()) // Redirect standard error
        .spawn()
        .expect("failed to execute process");

    let frames = get_framecount_tag(&file).unwrap_or_else(|_| {
        get_framecount_metadata(&file).unwrap_or_else(|_| {
            get_framecount_ffmpeg(&file)
                .unwrap_or_else(|_| get_framecount(&file).unwrap_or_else(|_| 0.0))
        })
    });

    // set transcode_progress length to the file's number of frames'
    transcode_bar.set_length(frames as u64);

    // Get the input file size from file in MB
    let input_file_size = get_file_size(file).unwrap() / 1024.0 / 1024.0;

    // Set file_bar message to the current file count / total file count + file name
    let file_name = Path::new(&file)
        .file_name()
        .unwrap_or(std::ffi::OsStr::new("Unknown"))
        .to_str()
        .unwrap_or("Invalid UTF-8");
    file_bar.set_message(format!("[{}]", file_name));

    loop {
        let mut buffer = [0; 1024]; // Adjust buffer size as needed
        match output.stderr.as_mut().unwrap().read(&mut buffer) {
            Ok(0) => {
                break;
            }
            Ok(n) => {
                let output_str = String::from_utf8_lossy(&buffer[..n]).trim().to_string();
                if let Some(frame) = parse_frame_from_output(&output_str) {
                    let frame = frame as u64; // convert frame to u64
                    transcode_bar.set_position(frame);
                    // TODO: If total files = 1 the iter, otherwise for loop
                    let mut current_frame_count = 0;
                    if *total_files == 1 {
                        current_frame_count = vector_files_to_process_frame_count
                            .iter()
                            .filter(|(file, _)| file == &file.to_string())
                            .map(|(_, frame_count)| frame_count)
                            .sum();
                    } else {
                        for i in 0..*current_file_count {
                            current_frame_count +=
                                vector_files_to_process_frame_count[i as usize].1;
                        }
                    }
                    // Set the total_bar position to the current frame count
                    total_bar.set_position(current_frame_count + frame);
                    total_bar.set_message(format!("{}/{}", current_file_count, total_files));
                    // set info_bar message to the current file count / total file count, current FPS from the output, and the file name
                    if let Some(fps) = parse_fps_from_output(&output_str) {
                        // Calculate size reduction in percentage, it can be taken from output_str being in the format of "frame=  100 fps=  0 q=-0.0 Lsize=     256kB time=00:00:04.00 bitrate= 524.3kbits/s speed=  20x"
                        // Get the size from output_str
                        let size_str: &str;
                        if output_str.contains("size=") {
                            size_str = output_str.split("size=").collect::<Vec<&str>>()[1];
                        } else {
                            // Handle the case where "size=" is not in the string
                            // For example, you might want to set size_str to a default value
                            size_str = "";
                        }

                        // Get the size in bytes
                        let size_bytes = size_str.split("time=").collect::<Vec<&str>>()[0];

                        // Trim the size_bytes and remove the "kB" from the end
                        let size_bytes = size_bytes.trim().replace("kB", "");

                        // Convert size_bytes to f32
                        let size_bytes = size_bytes.parse::<f32>().unwrap_or(0.0);

                        // Convert size_bytes to MB
                        let size_mb = size_bytes / 1024.0;

                        // Get the expected size of the output file in MB
                        // This can be calculates as the given size in output_str at the given frame / total frames * input_file_size
                        let expected_size_mb = (size_mb / (frame as f32)) * (frames as f32);

                        // Calcluate the expected percentage of the output file based on expected_size_mb
                        let expected_size_percent = (expected_size_mb / input_file_size) * 100.0;

                        // Show the speed in the info_bar
                        let speed = output_str
                            .find("speed=")
                            .map(|index| &output_str[index + 6..])
                            .and_then(|speed_str| {
                                speed_str.find('x').map(|index| &speed_str[..index])
                            })
                            .unwrap_or("");

                        info_bar.set_message(format!(
                            "{}][{}/{}][CRF: {}][VMAF: {}][{} FPS][{:.2} MB][{:.2}%][{}x",
                            transcode_info,
                            current_file_count,
                            total_files,
                            target_crf,
                            final_vmaf,
                            fps,
                            expected_size_mb,
                            expected_size_percent,
                            speed
                        ));

                        codec_bar
                            .set_message(format!("{}][{}", final_video_codec, final_audio_codec));

                        let _web_task_id = task_id.to_string();

                        *WEB_TASK_ID_STATIC.lock().unwrap() = _web_task_id.clone();
                        *WEB_FPS_STATIC.lock().unwrap() = fps.clone();
                        _web_fps = *WEB_FPS_STATIC.lock().unwrap();
                        *WEB_CURRENT_FRAME_STATIC.lock().unwrap() = frame.clone();
                        _web_current_frame = *WEB_CURRENT_FRAME_STATIC.lock().unwrap();
                        *WEB_TOTAL_FRAME_STATIC.lock().unwrap() = frames.clone();
                        _web_total_frame = *WEB_TOTAL_FRAME_STATIC.lock().unwrap();
                        *WEB_EXPECTED_SIZE_STATIC.lock().unwrap() = expected_size_mb.clone();
                        _web_expected_size = *WEB_EXPECTED_SIZE_STATIC.lock().unwrap();
                        *WEB_CURRENT_FILE_STATIC.lock().unwrap() = _web_current_file.clone();
                        *WEB_TOTAL_FILES_STATIC.lock().unwrap() = _web_total_files.clone();
                        *WEB_CURRENT_FILE_NAME_STATIC.lock().unwrap() =
                            _web_current_file_name.clone();

                        /*                         // Post progress to web server every 100ms in JSON format, add it to an existing array
                        let progress = Progress {
                            id: _web_task_id.clone(),
                            fps: _web_fps,
                            frame: _web_current_frame,
                            frames: _web_total_frame,
                            percentage: 0.0,
                            eta: "".to_string(),
                            size: _web_expected_size,
                            current_file_count: _web_current_file,
                            total_files: _web_total_files,
                            current_file_name: _web_current_file_name.to_string(),
                        };
                        let progress_json = serde_json::to_string(&progress).unwrap();

                        // Add the progress_json_array to the progress_json_array_string
                        let progress_json_array = format!("[{}]", progress_json);

                        // Post the progress_json_array_string to the web server
                        let _ = reqwest::Client::new()
                            .post(format!("http://localhost:8000/progress/{}", _web_task_id))
                            .body(progress_json_array.clone())
                            .send();

                        // TEMP Write the progress to a file
                        let _ = fs::write("progress.json", progress_json_array); */
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading stdout: {}", e);
                break;
            }
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }

    // Calculate and print size reduction in MB
    let input_file_size = get_file_size(file).unwrap() / 1024.0 / 1024.0;
    let output_file_size = get_file_size(output_folder).unwrap_or(0.0) / 1024.0 / 1024.0;
    let reduction = (1.0 - output_file_size / input_file_size) * 100.0;
    println!(
        "{}",
        format!(
            "Size reduction: {:.2} MB ({:.2}%)",
            input_file_size - output_file_size,
            reduction
        )
    );
}

pub fn run_ffmpeg_transcode_audio(
    file: &str,
    output_folder: &str,
    file_bar: &ProgressBar,
    transcode_bar: &ProgressBar,
    total_bar: &ProgressBar,
    info_bar: &ProgressBar,
    codec_bar: &ProgressBar,
    total_files: &i32,
    current_file_count: &u64,
    vector_files_to_process_frame_count: &Vec<(String, u64)>,
    original_audio_codec: &str,
    transcode_info: &str,
    vec_audio_args: &Vec<(usize, String, String)>,
    vec_video_args: &Vec<(usize, String, String, String)>,
    task_id: &str,
) {
    let _final_audio_codec: String;
    let _final_video_codec: String;
    let mut _web_task_id = task_id.to_string();
    let mut _web_fps: u64 = 0;
    let mut _web_current_frame: u64 = 0;
    let mut _web_total_frame: f32 = 0.0;
    let mut _web_expected_size: f32 = 0.0;
    let mut _web_current_file: u64 = current_file_count.clone() as u64;
    let mut _web_total_files: u64 = total_files.clone() as u64;
    let mut _web_current_file_name = file;

    // Prepare ffmpeg command
    let mut cmd = Command::new("ffmpeg.exe");

    cmd.arg("-y").arg("-i").arg(file);

    // map video stream with -map 0:v:0
    cmd.arg("-map").arg("0:v:0");

    // count how many audio streams there are
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let temp_output = output.clone();
    let audio_stream = String::from_utf8(temp_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    let audio_streams_count = audio_stream.lines().count() as i32;

    // count how many subtitle streams there are
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("s")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let temp_output = output.clone();
    let subtitle_stream = String::from_utf8(temp_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    let subtitle_streams_count = subtitle_stream.lines().count() as i32;

    // map all audio streams with -map 0:a copy, loop over with audio_streams_count
    for i in 0..audio_streams_count {
        cmd.arg("-map").arg(format!("0:a:{}", i));
        cmd.arg(format!("-c:a:{}", i)).arg("copy"); // Corrected syntax here
    }

    // map all subtitle streams with -map 0:a copy, loop over with subtitle_streams_count
    for i in 0..subtitle_streams_count {
        cmd.arg("-map").arg(format!("0:s:{}", i));
        cmd.arg(format!("-c:s:{}", i)).arg("copy"); // Corrected syntax here
    }

    let metadata = run_ffmpeg_map_metadata(file);

    if metadata != "" {
        cmd.arg("-map_metadata").arg("-1");
        for arg in metadata.split_whitespace() {
            cmd.arg(arg);
        }
    } else {
        cmd.arg("-map_metadata").arg("0");
    }

    let mut final_audio_codec = String::new(); // Initialize with an empty string
    if !vec_audio_args.is_empty() {
        for (i, arg, codec) in vec_audio_args {
            if !arg.is_empty() {
                for arg in arg.split_whitespace() {
                    cmd.arg(arg);
                }
                if codec != "opus" || codec != "aac" || codec != "mp3" {
                    final_audio_codec.push_str(&format!("{}->{},", codec, "opus"));
                } else {
                    final_audio_codec.push_str(&format!("{},", codec));
                }
            } else if arg == "" {
                cmd.arg(format!("-c:a:{}", i)).arg("copy");
                if codec != "opus" || codec != "aac" || codec != "mp3" {
                    final_audio_codec.push_str(&format!("{}->{},", codec, "opus"));
                } else {
                    final_audio_codec.push_str(&format!("{},", codec));
                }
            }
        }
        final_audio_codec.pop(); // Remove the trailing comma
    } else {
        final_audio_codec = format!("{}->{}", original_audio_codec, "opus");
    }

    // Add video codec to final_video_codec string
    let mut final_video_codec = String::new(); // Initialize with an empty string
    if !vec_video_args.is_empty() {
        for (_i, codec, _width, _height) in vec_video_args {
            if !codec.is_empty() {
                final_video_codec.push_str(&format!("{},", codec));
            }
        }
        final_video_codec.pop(); // Remove the trailing comma
    } else {
        final_video_codec = "copy".to_string();
    }

    cmd.arg("-vcodec")
        .arg("copy")
        .arg(Path::new(&output_folder));

    // Execute ffmpeg command
    let mut output = cmd
        .stdout(Stdio::piped()) // Redirect standard output
        .stderr(Stdio::piped()) // Redirect standard error
        .spawn()
        .expect("failed to execute process");

    let frames = get_framecount_tag(&file).unwrap_or_else(|_| {
        get_framecount_metadata(&file).unwrap_or_else(|_| {
            get_framecount_ffmpeg(&file)
                .unwrap_or_else(|_| get_framecount(&file).unwrap_or_else(|_| 0.0))
        })
    });

    // set transcode_progress length to the file's number of frames'
    transcode_bar.set_length(frames as u64);

    // Get the input file size from file in MB
    let input_file_size = get_file_size(file).unwrap() / 1024.0 / 1024.0;

    // Set file_bar message to the current file count / total file count + file name
    let file_name = Path::new(&file)
        .file_name()
        .unwrap_or(std::ffi::OsStr::new("Unknown"))
        .to_str()
        .unwrap_or("Invalid UTF-8");
    file_bar.set_message(format!("[{}]", file_name));

    loop {
        let mut buffer = [0; 1024]; // Adjust buffer size as needed
        match output.stderr.as_mut().unwrap().read(&mut buffer) {
            Ok(0) => {
                break;
            }
            Ok(n) => {
                let output_str = String::from_utf8_lossy(&buffer[..n]).trim().to_string();
                if let Some(frame) = parse_frame_from_output(&output_str) {
                    let frame = frame as u64; // convert frame to u64
                    transcode_bar.set_position(frame);
                    let mut current_frame_count = 0;
                    for i in 0..*current_file_count {
                        current_frame_count += vector_files_to_process_frame_count[i as usize].1;
                    }
                    // Set the total_bar position to the current frame count
                    total_bar.set_position(current_frame_count + frame);
                    total_bar.set_message(format!("{}/{}", current_file_count, total_files));
                    // set info_bar message to the current file count / total file count, current FPS from the output, and the file name
                    if let Some(fps) = parse_fps_from_output(&output_str) {
                        // Calculate size reduction in percentage, it can be taken from output_str being in the format of "frame=  100 fps=  0 q=-0.0 Lsize=     256kB time=00:00:04.00 bitrate= 524.3kbits/s speed=  20x"
                        // Get the size from output_str
                        let size_str: &str;
                        if output_str.contains("size=") {
                            size_str = output_str.split("size=").collect::<Vec<&str>>()[1];
                        } else {
                            // Handle the case where "size=" is not in the string
                            // For example, you might want to set size_str to a default value
                            size_str = "";
                        }

                        // Get the size in bytes
                        let size_bytes = size_str.split("time=").collect::<Vec<&str>>()[0];

                        // Trim the size_bytes and remove the "kB" from the end
                        let size_bytes = size_bytes.trim().replace("kB", "");

                        // Convert size_bytes to f32
                        let size_bytes = size_bytes.parse::<f32>().unwrap_or(0.0);

                        // Convert size_bytes to MB
                        let size_mb = size_bytes / 1024.0;

                        // Get the expected size of the output file in MB
                        // This can be calculates as the given size in output_str at the given frame / total frames * input_file_size
                        let expected_size_mb = (size_mb / (frame as f32)) * (frames as f32);

                        // Calcluate the expected percentage of the output file based on expected_size_mb
                        let expected_size_percent = (expected_size_mb / input_file_size) * 100.0;

                        // Show the speed in the info_bar
                        let speed = output_str
                            .find("speed=")
                            .map(|index| &output_str[index + 6..])
                            .and_then(|speed_str| {
                                speed_str.find('x').map(|index| &speed_str[..index])
                            })
                            .unwrap_or("");

                        info_bar.set_message(format!(
                            "{}][{}/{}][{} FPS][{:.2} MB][{:.2}%][{}x",
                            transcode_info,
                            current_file_count,
                            total_files,
                            fps,
                            expected_size_mb,
                            expected_size_percent,
                            speed
                        ));

                        codec_bar
                            .set_message(format!("{}][{}", final_video_codec, final_audio_codec));

                        *WEB_TASK_ID_STATIC.lock().unwrap() = _web_task_id.clone();
                        *WEB_FPS_STATIC.lock().unwrap() = fps.clone();
                        _web_fps = *WEB_FPS_STATIC.lock().unwrap();
                        *WEB_CURRENT_FRAME_STATIC.lock().unwrap() = frame.clone();
                        _web_current_frame = *WEB_CURRENT_FRAME_STATIC.lock().unwrap();
                        *WEB_TOTAL_FRAME_STATIC.lock().unwrap() = frames.clone();
                        _web_total_frame = *WEB_TOTAL_FRAME_STATIC.lock().unwrap();
                        *WEB_EXPECTED_SIZE_STATIC.lock().unwrap() = expected_size_mb.clone();
                        _web_expected_size = *WEB_EXPECTED_SIZE_STATIC.lock().unwrap();
                        *WEB_CURRENT_FILE_STATIC.lock().unwrap() = _web_current_file.clone();
                        *WEB_TOTAL_FILES_STATIC.lock().unwrap() = _web_total_files.clone();
                        *WEB_CURRENT_FILE_NAME_STATIC.lock().unwrap() =
                            _web_current_file_name.to_string();

                        /*                         // Post progress to web server every 100ms in JSON format, add it to an existing array
                        let progress = Progress {
                            id: _web_task_id.clone(),
                            fps: _web_fps,
                            frame: _web_current_frame,
                            frames: _web_total_frame,
                            percentage: 0.0,
                            eta: "".to_string(),
                            size: _web_expected_size,
                            current_file_count: _web_current_file,
                            total_files: _web_total_files,
                            current_file_name: _web_current_file_name.to_string(),
                        };
                        let progress_json = serde_json::to_string(&progress).unwrap();

                        // Add the progress_json_array to the progress_json_array_string
                        let progress_json_array = format!("[{}]", progress_json);

                        // Post the progress_json_array_string to the web server
                        let _ = reqwest::Client::new()
                            .post(format!("http://localhost:8000/progress/{}", _web_task_id))
                            .body(progress_json_array.clone())
                            .send();

                        // TEMP Write the progress to a file
                        let _ = fs::write("progress.json", progress_json_array); */
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading stdout: {}", e);
                break;
            }
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }

    // Calculate and print size reduction in MB
    let input_file_size = get_file_size(file).unwrap() / 1024.0 / 1024.0;
    let output_file_size = get_file_size(output_folder).unwrap() / 1024.0 / 1024.0;
    let reduction = (1.0 - output_file_size / input_file_size) * 100.0;
    println!(
        "{}",
        format!(
            "Size reduction: {:.2} MB ({:.2}%)",
            input_file_size - output_file_size,
            reduction
        )
    );
}

pub fn get_progress_web() -> Progress {
    // Mock data for demonstration purposes
    // In a real-world scenario, replace this with actual data from your transcoding process

    // Simulate FPS based on current second
    let task_id = WEB_TASK_ID_STATIC.lock().unwrap();
    let fps = WEB_FPS_STATIC.lock().unwrap();
    let frame = WEB_CURRENT_FRAME_STATIC.lock().unwrap();
    let frames = WEB_TOTAL_FRAME_STATIC.lock().unwrap();
    let percentage = (*frame as f32 / *frames as f32) * 100.0;
    let size = WEB_EXPECTED_SIZE_STATIC.lock().unwrap();
    // Calculate eta based on fps and frame/frames

    //let eta = calculate_eta(*fps, *frame, *frames);
    let eta = "".to_string();

    let current_file_count = WEB_CURRENT_FILE_STATIC.lock().unwrap();
    let total_files = WEB_TOTAL_FILES_STATIC.lock().unwrap();
    let current_file_name = WEB_CURRENT_FILE_NAME_STATIC.lock().unwrap();
    let stem_filename_ = current_file_name.clone();

    // Get the stem of the file
    let stem_filename = Path::new(&stem_filename_)
        .file_stem()
        .unwrap_or(std::ffi::OsStr::new("Unknown"))
        .to_str()
        .unwrap_or("Invalid UTF-8");

    // If values are MAX of each type, set values in Progress to 0
    if *fps == u64::MAX {
        return Progress {
            id: "".to_string(),
            fps: 0,
            frame: 0,
            frames: 0.0,
            percentage: 0.0,
            eta: eta,
            size: 0.0,
            current_file_count: 0,
            total_files: 0,
            current_file_name: "Unknown".to_string(),
        };
    } else {
        Progress {
            id: task_id.to_string(),
            fps: *fps,
            frame: *frame,
            frames: *frames,
            percentage: percentage,
            eta: eta,
            size: *size,
            current_file_count: *current_file_count,
            total_files: *total_files,
            current_file_name: stem_filename.to_string(),
        }
    }
}

pub fn get_progress_web_id(id: String) -> Progress {
    // Mock data for demonstration purposes
    // In a real-world scenario, replace this with actual data from your transcoding process

    // Simulate FPS based on current second
    let task_id = id;
    let fps = WEB_FPS_STATIC.lock().unwrap();
    let frame = WEB_CURRENT_FRAME_STATIC.lock().unwrap();
    let frames = WEB_TOTAL_FRAME_STATIC.lock().unwrap();
    let percentage = (*frame as f32 / *frames as f32) * 100.0;
    let size = WEB_EXPECTED_SIZE_STATIC.lock().unwrap();
    // Calculate eta based on fps and frame/frames

    //let eta = calculate_eta(*fps, *frame, *frames);
    let eta = "".to_string();

    let current_file_count = WEB_CURRENT_FILE_STATIC.lock().unwrap();
    let total_files = WEB_TOTAL_FILES_STATIC.lock().unwrap();
    let current_file_name = WEB_CURRENT_FILE_NAME_STATIC.lock().unwrap();
    let stem_filename_ = current_file_name.clone();

    // Get the stem of the file
    let stem_filename = Path::new(&stem_filename_)
        .file_stem()
        .unwrap_or(std::ffi::OsStr::new("Unknown"))
        .to_str()
        .unwrap_or("Invalid UTF-8");

    // If values are MAX of each type, set values in Progress to 0
    if *fps == u64::MAX {
        return Progress {
            id: "".to_string(),
            fps: 0,
            frame: 0,
            frames: 0.0,
            percentage: 0.0,
            eta: eta,
            size: 0.0,
            current_file_count: 0,
            total_files: 0,
            current_file_name: "Unknown".to_string(),
        };
    } else {
        Progress {
            id: task_id.to_string(),
            fps: *fps,
            frame: *frame,
            frames: *frames,
            percentage: percentage,
            eta: eta,
            size: *size,
            current_file_count: *current_file_count,
            total_files: *total_files,
            current_file_name: stem_filename.to_string(),
        }
    }
}

pub fn get_progress_scan_web() -> ProgressScan {
    // Mock data for demonstration purposes
    // In a real-world scenario, replace this with actual data from your transcoding process

    // Simulate FPS based on current second
    let count = WEB_SCAN_COUNT_STATIC.lock().unwrap();
    let total = WEB_SCAN_TOTAL_STATIC.lock().unwrap();

    // If values are MAX of each type, set values in Progress to 0
    if *count == u64::MAX {
        return ProgressScan { count: 0, total: 0 };
    } else {
        ProgressScan {
            count: *count,
            total: *total,
        }
    }
}

/* fn calculate_eta(fps: u64, frame: u64, _frames: f32) -> String {
    let seconds = frame / fps;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    let hours = minutes / 60;
    let minutes = minutes % 60;
    let eta = format!("{}:{:02}:{:02}", hours, minutes, seconds);
    eta
} */

// Get all the items from the db
pub fn get_all_from_db() -> Result<
    Vec<(
        i32,
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )>,
> {
    let conn = Connection::open("data.db")?;
    let mut stmt = conn.prepare("SELECT * FROM video_info")?;
    let mut rows = stmt
        .query_map(params![], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
                row.get(17)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(
        i32,
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }
    Ok(db_items)
}

// Get all the items from the db that match the given input
pub fn get_all_from_db_search(
    search: &str,
) -> Result<
    Vec<(
        i32,
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )>,
> {
    let conn = Connection::open("data.db")?;
    let mut stmt = conn.prepare("SELECT * FROM video_info WHERE filepath LIKE '%' || ?1 || '%'")?;
    let mut rows = stmt
        .query_map(params![search], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
                row.get(17)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(
        i32,
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }
    Ok(db_items)
}

pub fn add_to_db(
    files: Vec<String>,
    bar: ProgressBar,
) -> Result<(Vec<AtomicI32>, Arc<Mutex<Vec<std::string::String>>>)> {
    let count: AtomicI32 = AtomicI32::new(0);
    let db_count;
    let db_count_added: AtomicI32 = AtomicI32::new(0);
    let db_count_skipped: AtomicI32 = AtomicI32::new(0);
    let files_to_process: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let conn = Connection::open("data.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS video_info (
                    id INTEGER PRIMARY KEY,
                    filename TEXT NOT NULL,
                    filepath TEXT NOT NULL,
                    width INTEGER NOT NULL,
                    height INTEGER NOT NULL,
                    duration REAL NOT NULL,
                    pixel_format TEXT NOT NULL,
                    display_aspect_ratio TEXT NOT NULL,
                    sample_aspect_ratio TEXT NOT NULL,
                    format TEXT NOT NULL,
                    size BIGINT NOT NULL,
                    folder_size BIGINT NOT NULL,
                    bitrate BIGINT NOT NULL,
                    codec TEXT NOT NULL,
                    status TEXT NOT NULL,
                    audio_codec TEXT NOT NULL,
                    audio_bitrate BIGINT NOT NULL,
                    hash TEXT NOT NULL
                  )",
        params![],
    )?;

    let filenames_skip = files.clone();
    let filenames_audio = files.clone();
    let mut filenames = files;

    // get all items in db
    let mut stmt = conn.prepare("SELECT * FROM video_info")?;
    let mut rows = stmt
        .query_map(params![], |row| {
            Ok((
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
                row.get(17)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }

    // get all items from filenames that are not in db
    let mut filenames_to_process: Vec<String> = Vec::new();
    for filename in filenames {
        let real_filename = Path::new(&filename).file_name().unwrap().to_str().unwrap();
        let mut found = false;
        for item in &db_items {
            if item.0 == real_filename {
                found = true;
                break;
            }
        }
        if !found {
            filenames_to_process.push(filename);
        }
    }

    // get all the items from filenames that are in db
    let mut filenames_to_skip: Vec<String> = Vec::new();
    for filename in filenames_skip {
        let real_filename = Path::new(&filename).file_name().unwrap().to_str().unwrap();
        let mut found = false;
        for item in &db_items {
            if item.0 == real_filename {
                found = true;
                break;
            }
        }
        if found {
            filenames_to_skip.push(filename);
        }
    }
    db_count = AtomicI32::new(filenames_to_skip.len() as i32);

    // setup progress bar exists_bar and set the length to the count of all items in db
    let exists_bar = ProgressBar::new_spinner();
    let exists_style =
        "[exis][{elapsed_precise}][{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} [analyzed files] eta: {eta:<7}";
    exists_bar.set_style(
        ProgressStyle::default_spinner()
            .template(exists_style)
            .unwrap(),
    );
    // set exists_bar length to the number of items in db that match filenames in the given folder
    exists_bar.set_length(filenames_to_skip.len() as u64);

    // get all the items from filenames that are in db but do not exist anymore, wait for exists_bar to finish
    let mut filenames_to_remove_from_db: Vec<String> = Vec::new();
    for filename in filenames_to_skip {
        let real_filename = Path::new(&filename).file_name().unwrap().to_str().unwrap();
        let mut found = false;
        for item in &db_items {
            if item.0 == real_filename {
                found = true;
                break;
            }
        }
        if !found {
            filenames_to_remove_from_db.push(filename);
        }
        exists_bar.inc(1);
    }

    // print count for all items in db_count_to_remove_from_db
    println!(
        "Found {} files in database that do not exist anymore",
        filenames_to_remove_from_db.len()
    );

    /*     // list all the files that are in db but do not exist anymore
    if filenames_to_remove_from_db.len() > 0 {
        println!("Found {} files in database that do not exist anymore", filenames_to_remove_from_db.len());
        for filename in filenames_to_remove_from_db {
            println!("{}", filename);
        }
    } */

    let filenames_to_remove_from_db_count = filenames_to_remove_from_db.len() as i32;

    // remove all the items from db that do not exist anymore
    if filenames_to_remove_from_db.len() > 0 {
        for filename in filenames_to_remove_from_db {
            //TEMP print REMOVING: filename in yellow
            println!("\x1b[33mREMOVING: {}\x1b[0m", filename);
            let mut stmt = conn
                .prepare("DELETE FROM video_info WHERE filename=?1")
                .unwrap();
            stmt.execute(params![filename]).unwrap();
        }
    }

    if filenames_to_remove_from_db_count > 0 {
        // print "Removed <count> files from database"
        println!(
            "Removed {} files from database",
            filenames_to_remove_from_db_count
        );
    } else {
        // print "No files to remove from database"
        println!("No files to remove from database");
    }

    // print count for all items in filenames_to_process and return filenames with all items in db removed
    println!("Found {} files not in database", filenames_to_process.len());
    filenames = filenames_to_process.clone();

    let conn = Arc::new(Mutex::new(Connection::open("data.db")?));

    // get all the items from filenames that are in db that have audio_codec == "NaN" or audio_bitrate == 0
    let mut filenames_to_update: Vec<String> = Vec::new();
    for filename in filenames_audio {
        let real_filename = Path::new(&filename).file_name().unwrap().to_str().unwrap();
        let mut found = false;
        for item in &db_items {
            if item.0 == real_filename && (item.14 == "NaN" || item.15 == 0) {
                found = true;
                break;
            }
        }
        if found {
            filenames_to_update.push(filename);
        }
    }

    // print count for all items in filenames_to_update
    println!("Found {} files to update in db", filenames_to_update.len());
    if filenames_to_update.len() > 0 {
        filenames = filenames_to_update.clone();
        bar.set_length(filenames_to_update.len() as u64);
    }

    // if >0 files to process, set bar length to filenames_to_process.len()
    if filenames_to_process.len() > 0 {
        bar.set_length(filenames_to_process.len() as u64);
    }

    /*     // list files to update
    if filenames_to_update.len() > 0 {
        for filename in filenames_to_update {
            println!("{}", filename);
        }
    } */

    filenames.par_iter().for_each(|filename| {
        let real_filename = Path::new(filename).file_name().unwrap().to_str().unwrap();
        let conn = conn.clone();
        let conn = conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM video_info WHERE filename=?1").unwrap();
        let file_exists: bool = stmt.exists(params![real_filename]).unwrap();

        // TEMP print filename
        //println!("{}", filename);

        if !file_exists {
            let video_output = Command::new("ffprobe")
                .args([
                    "-i",
                    filename,
                    "-v",
                    "error",
                    "-select_streams",
                    "v",
                    "-show_entries",
                    "stream",
                    "-show_format",
                    "-show_data_hash",
                    "sha256",
                    "-show_streams",
                    "-of",
                    "json",
                ])
                .output()
                .expect("failed to execute process");
            let audio_output = Command::new("ffprobe")
                .args([
                    "-i",
                    filename,
                    "-v",
                    "error",
                    "-select_streams",
                    "a",
                    "-show_entries",
                    "stream",
                    "-show_format",
                    "-show_data_hash",
                    "sha256",
                    "-show_streams",
                    "-of",
                    "json",
                ])
                .output()
                .expect("failed to execute process");

            let audio_json_value: Value = serde_json
                ::from_str(&String::from_utf8_lossy(&audio_output.stdout))
                .unwrap();
            let audio_json_str = audio_json_value.to_string();
            let json_value: Value = serde_json
                ::from_str(&String::from_utf8_lossy(&video_output.stdout))
                .unwrap();
            let json_str = json_value.to_string();
            if &json_str.len() >= &1 && &audio_json_str.len() >= &1 {
                let audio_values: Value = audio_json_value;
                let audio_bitrate = audio_values["format"]["bit_rate"].as_str().unwrap_or("0");
                let audio_codec = audio_values["streams"][0]["codec_name"]
                    .as_str()
                    .unwrap_or("NaN");
                let values: Value = json_value;
                let _width = values["streams"][0]["width"].as_i64().unwrap_or(0);
                let _height = values["streams"][0]["height"].as_i64().unwrap_or(0);
                let filepath = values["format"]["filename"].as_str().unwrap();
                let filename = Path::new(filepath).file_name().unwrap().to_str().unwrap();
                let size = values["format"]["size"].as_str().unwrap_or("0");
                let bitrate = values["format"]["bit_rate"].as_str().unwrap_or("0");
                let duration = values["format"]["duration"].as_str().unwrap_or("0.0");
                let format = values["format"]["format_name"].as_str().unwrap_or("NaN");
                let width = values["streams"][0]["width"].as_i64().unwrap_or(0);
                let height = values["streams"][0]["height"].as_i64().unwrap_or(0);
                let codec = values["streams"][0]["codec_name"].as_str().unwrap_or("NaN");
                let pix_fmt = values["streams"][0]["pix_fmt"].as_str().unwrap_or("NaN");
                let checksum = values["streams"][0]["extradata_hash"].as_str().unwrap_or("NaN");
                let dar = values["streams"][0]["display_aspect_ratio"].as_str().unwrap_or("NaN");
                let sar = values["streams"][0]["sample_aspect_ratio"].as_str().unwrap_or("NaN");

                // for each file in this folder and it's subfodlers, sum the size of the files
                let mut folder_size = 0;
                for entry in WalkDir::new(Path::new(filepath).parent().unwrap()) {
                    let entry = entry.unwrap();
                    let metadata = fs::metadata(entry.path());
                    folder_size += metadata.unwrap().len() as i64;
                }

                // if bitrate is over 6MB/s then add to db with status pending_video, otherwise add to db with status skipped
                // also if audio_codec is not aac or opus then add to db with status pending_audio, otherwise add to db with status skipped
                // if both video and audio are pending then add to db with status pending_all
                let mut status = "pending_video";
                if bitrate.parse::<i64>().unwrap() < 6000000 {
                    status = "skipped";
                }
                if bitrate.parse::<i64>().unwrap() > 6000000 {
                    status = "pending_video";
                }
                if audio_codec != "aac" && audio_codec != "opus" && audio_codec != "mp3" {
                    status = "pending_audio";
                }
                if
                    bitrate.parse::<i64>().unwrap() > 6000000 &&
                    audio_codec != "aac" &&
                    audio_codec != "opus" &&
                    audio_codec != "mp3"
                {
                    status = "pending_all";
                }

                conn.execute(
                    "INSERT INTO video_info (filename, filepath, width, height, duration, pixel_format, display_aspect_ratio, sample_aspect_ratio, format, size, folder_size, bitrate, codec, status, audio_codec, audio_bitrate, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                    params![
                        filename,
                        filepath,
                        width,
                        height,
                        duration,
                        pix_fmt,
                        dar,
                        sar,
                        format,
                        size,
                        folder_size,
                        bitrate,
                        codec,
                        status,
                        audio_codec,
                        audio_bitrate,
                        checksum
                    ]
                ).unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                db_count_added.fetch_add(1, Ordering::SeqCst);
            }
        }
        bar.inc(1);
    });

    // return all the counters
    Ok((
        vec![count, db_count, db_count_added, db_count_skipped],
        files_to_process,
    ))
}

// Function to add the given files to the db in a table called db_queue
pub fn add_to_db_queue(
    input_path: &str,
    output_path: &str,
    encoder: &str,
    preset: &str,
    vmaf_target: &str,
    vmaf_threads: &str,
) {
    let conn = Connection::open("data.db").unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS db_queue (
                    id INTEGER PRIMARY KEY,
                    input_path TEXT NOT NULL,
                    output_path TEXT NOT NULL,
                    encoder TEXT NOT NULL,
                    preset TEXT NOT NULL,
                    vmaf_target TEXT NOT NULL,
                    vmaf_threads TEXT NOT NULL
                  )",
        params![],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO db_queue (input_path, output_path, encoder, preset, vmaf_target, vmaf_threads) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![input_path, output_path, encoder, preset, vmaf_target, vmaf_threads]
    ).unwrap();
}

// function to remove item from db_queue
pub fn remove_from_db_queue(id: String) -> Result<()> {
    let conn = Connection::open("data.db")?;
    let mut stmt = conn.prepare("DELETE FROM db_queue WHERE id=?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

// Function to get all the items from the db_queue table
pub fn get_all_from_db_queue() -> Result<Vec<(i32, String, String, String, String, String, String)>>
{
    let conn = Connection::open("data.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS db_queue (
                    id INTEGER PRIMARY KEY,
                    input_path TEXT NOT NULL,
                    output_path TEXT NOT NULL,
                    encoder TEXT NOT NULL,
                    preset TEXT NOT NULL,
                    vmaf_target TEXT NOT NULL,
                    vmaf_threads TEXT NOT NULL
                  )",
        params![],
    )
    .unwrap();
    let mut stmt = conn.prepare("SELECT * FROM db_queue")?;
    let mut rows = stmt
        .query_map(params![], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(i32, String, String, String, String, String, String)> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }
    Ok(db_items)
}

// function to remove items from db that don't exists anymore
/* fn remove_from_db() -> Result<()> {
    let conn = Connection::open("data.db")?;
    let mut stmt = conn.prepare("SELECT * FROM video_info")?;
    let mut rows = stmt
        .query_map(params![], |row| {
            Ok((
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
                row.get(17)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }

    let mut filenames_to_remove: Vec<String> = Vec::new();
    let remove_bar = ProgressBar::new_spinner();
    let remove_style =
        "[remo][{elapsed_precise}][{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} eta: {eta:<7}";
    remove_bar.set_style(
        ProgressStyle::default_spinner()
            .template(remove_style)
            .unwrap(),
    );

    remove_bar.set_length(db_items.len() as u64);

    for item in &db_items {
        let real_filename = Path::new(&item.1).file_name().unwrap().to_str().unwrap();
        let file_exists = metadata(&item.1).is_ok();
        if !file_exists {
            filenames_to_remove.push(real_filename.to_string());
        }
        remove_bar.inc(1);
    }
    remove_bar.finish();

    for filename in filenames_to_remove {
        let mut stmt = conn
            .prepare("DELETE FROM video_info WHERE filename=?1")
            .unwrap();
        stmt.execute(params![filename]).unwrap();
    }

    Ok(())
} */

// function to remove items from db that don't exists anymore, but only for the specified folder and it's subfolders
pub fn remove_from_db_folder(folder: &str) -> Result<()> {
    let conn = Connection::open("data.db")?;
    let mut stmt = conn.prepare("SELECT * FROM video_info WHERE filepath LIKE '%' || ?1 || '%'")?;
    let mut rows = stmt
        .query_map(params![folder], |row| {
            Ok((
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
                row.get(17)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
    )> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }

    let mut filenames_to_remove: Vec<String> = Vec::new();
    let remove_bar = ProgressBar::new_spinner();
    let remove_style =
        "[remo][{elapsed_precise}][{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} eta: {eta:<7}";
    remove_bar.set_style(
        ProgressStyle::default_spinner()
            .template(remove_style)
            .unwrap(),
    );

    remove_bar.set_length(db_items.len() as u64);

    for item in &db_items {
        let real_filename = Path::new(&item.1).file_name().unwrap().to_str().unwrap();
        let file_exists = metadata(&item.1).is_ok();
        if !file_exists {
            filenames_to_remove.push(real_filename.to_string());
        }
        remove_bar.inc(1);
    }
    remove_bar.finish();

    for filename in filenames_to_remove {
        let mut stmt = conn
            .prepare("DELETE FROM video_info WHERE filename=?1")
            .unwrap();
        stmt.execute(params![filename]).unwrap();
        // TEMP print REMOVING: filename in yellow
        println!("\x1b[33mREMOVING: {}\x1b[0m", filename);
    }

    Ok(())
}

//[out#0/nut @ 000001e76b343b40] video:470kB audio:0kB
fn parse_size_output(output: &Output) -> Option<String> {
    let output_str_stderr = String::from_utf8_lossy(&output.stderr);
    for line in output_str_stderr.lines() {
        if line.contains("[out#0/null") {
            return line.split("video:").nth(1)?.trim().parse().ok();
        }
    }
    None
}

//[out#0/nut @ 000001e76b343b40] video:470kB audio:0kB
fn parse_encode_size_output(line: &String) -> Option<String> {
    if line.contains("[out#0/") {
        return line.split("video:").nth(1)?.trim().parse().ok();
    }
    None
}

fn process_video_pipe_and_vmaf(
    file: &String,
    args: &Args,
    crf: &f32,
    fps: &str,
    ss_arg: &String,
    to_arg: &String,
    vmaf_pool: &String,
    vmaf_threads: &str,
    vmaf_subsample: &str,
) -> Result<Output, io::Error> {
    // set preset to the preset linked to encoder
    let preset = match args.encoder.as_str() {
        "libx265" => &args.preset_x265,
        "hevc_nvenc" => &args.preset_hevc_nvenc,
        "hevc_qsv" => &args.preset_hevc_qsv,
        "av1" => &args.preset_libaom_av1,
        "av1_qsv" => &args.preset_av1_qsv,
        "libsvtav1" => &args.preset_libsvtav1,
        _ => &args.preset_x265,
    };

    // set params to the params linked to encoder
    let params = match args.encoder.as_str() {
        "libx265" => &args.params_x265,
        "hevc_nvenc" => &args.params_hevc_nvenc,
        "hevc_qsv" => &args.params_hevc_qsv,
        "av1" => &args.params_libaom_av1,
        "av1_qsv" => &args.params_av1_qsv,
        "libsvtav1" => &args.params_libsvtav1,
        _ => &args.params_x265,
    };

    let mut encode_command = Command::new("./ffmpeg.exe");
    encode_command
        .arg("-y")
        .arg("-r")
        .arg(fps)
        .arg("-ss")
        .arg(&ss_arg)
        .arg("-to")
        .arg(&to_arg)
        .arg("-an")
        .arg("-sn")
        .arg("-dn")
        .arg("-i")
        .arg(file)
        .arg("-c:v")
        .arg(&args.encoder)
        .arg("-preset")
        .arg(&preset);
    // for each parameter in params separated by space add it
    for param in params.split(' ') {
        encode_command.arg(param);
    }

    if args.encoder == "hevc_nvenc" {
        encode_command
            .arg("-rc:v")
            .arg("vbr")
            .arg("-cq:v")
            .arg(crf.to_string())
            .arg("-qmin")
            .arg(crf.to_string())
            .arg("-qmax")
            .arg(crf.to_string());
    } else if args.encoder == "hevc_qsv" {
        encode_command.arg("-global_quality").arg(crf.to_string());
    } else if args.encoder == "libx265" {
        encode_command.arg("-crf").arg(crf.to_string());
    } else if args.encoder == "av1" {
    }

    encode_command
        .arg("-pix_fmt")
        .arg("yuv420p10le")
        .arg("-f")
        .arg("nut")
        .arg("pipe:1");

    encode_command.stdout(Stdio::piped());

    // TEMP print command
    //println!("{:?}", encode_command);
    // print command as it will be executed
    //println!("{}", encode_command.get_program().to_str().unwrap());
    //println!("{}", encode_command.get_args().map(|a| a.to_str().unwrap()).collect::<Vec<&str>>().join(" "));
    //exit(1);

    let encode_process = encode_command.stderr(Stdio::null()).spawn()?;

    let mut vmaf_command = Command::new("ffmpeg");
    vmaf_command.args([
        "-r" , fps, "-ss", &ss_arg, "-to", &to_arg,
        "-an", "-sn", "-dn",
        "-i", file, // Reference file
        "-thread_queue_size", "4096",
        "-f", "nut", "-i", "pipe:0", // Reading from pipe
        //"-lavfi", &format!("[0:v]setpts=PTS-STARTPTS,fps={}[reference];[1:v]setpts=PTS-STARTPTS,fps={}[distorted];[reference][distorted]libvmaf='pool={}:n_threads={}:n_subsample={}'",&fps,&fps,&vmaf_pool,&vmaf_threads, &vmaf_subsample),
        "-lavfi", &format!("[0:v]setpts=PTS-STARTPTS[reference];[1:v]setpts=PTS-STARTPTS[distorted];[reference][distorted]libvmaf='pool={}:n_threads={}:n_subsample={}'",&vmaf_pool,&vmaf_threads, &vmaf_subsample),
        "-f", "null", "-"
    ]);

    // Take the stdout from the first process as stdin for the VMAF calculation
    if let Some(encode_stdout) = encode_process.stdout {
        vmaf_command.stdin(Stdio::from(encode_stdout));
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to pipe output from encoding process",
        ));
    }

    // Set the stderr to be captured for the VMAF calculation process
    // Assuming the VMAF score or relevant log will be printed to stderr
    vmaf_command.stdout(Stdio::piped());
    vmaf_command.stderr(Stdio::piped());

    // Spawn the VMAF calculation process
    let vmaf_process = vmaf_command.spawn()?;

    //TEMP
    let output_test = vmaf_process.wait_with_output()?;
    //println!("{}", String::from_utf8_lossy(&output_test.stdout));

    Ok(output_test)
}

pub fn get_scene_size(file_path: &str, ss: &str, to: &str) -> Result<i32, Error> {
    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-an")
        .arg("-dn")
        .arg("-sn")
        .arg("-ss")
        .arg(ss)
        .arg("-to")
        .arg(to)
        .arg("-i")
        .arg(file_path)
        .arg("-c:v")
        .arg("copy")
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .expect("failed to execute ffprobe");

    let scene_size_output = parse_size_output(&output).unwrap();
    let pattern = Regex::new(r"\b\d+kB\b").unwrap();
    if let Some(matched) = pattern.find(&scene_size_output) {
        // get only the numbers from the matched size string
        let numbers = &scene_size_output[matched.start()..matched.end()].replace("kB", "");
        let number = numbers.trim().parse::<i32>().unwrap();
        Ok(number)
    } else {
        Ok(0)
    }
}

// TO FIX
/* fn update_db_audio_info(
    conn: &Connection,
    filename: &str,
    audio_bitrate: &str,
    audio_codec: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt =
        conn.prepare("UPDATE video_info SET audio_bitrate=?1, audio_codec=?2 WHERE filepath=?3")?;
    let result = stmt.execute(params![audio_bitrate, audio_codec, filename])?;
    if result == 0 {
        Err(rusqlite::Error::QueryReturnedNoRows)
    } else {
        Ok(())
    }
} */

/* fn update_db_status(
    conn: &Connection,
    filepath: &str,
    status: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("UPDATE video_info SET status=?1 WHERE filepath=?2")?;
    stmt.execute(params![status, filepath])?;
    Ok(())
} */

/* // Function to update the db with the video info if the folder size has changed
// First get the folder size from the db, then get the folder size from the filesystem, then compare the two
// If the folder size has changed, then the video info has changed, so update the db with the new video info
fn update_db_info(
    conn: &Connection,
    filename: &str,
    filepath: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT folder_size FROM video_info WHERE filename=?1")?;
    let mut rows = stmt.query_map(params![filename], |row| row.get(usize::from(0)))?;
    let mut folder_size: i64 = 0;
    while let Some(row) = rows.next() {
        folder_size = row.unwrap().unwrap();
    }

    let mut new_folder_size: i64 = 0;
    for entry in WalkDir::new(Path::new(filepath).parent().unwrap()) {
        let entry = entry.unwrap();
        let metadata = fs::metadata(entry.path());
        new_folder_size += metadata.unwrap().len() as i64;
    }

    if folder_size != new_folder_size {
        let video_output = Command::new("ffprobe")
            .args([
                "-i",
                filepath,
                "-v",
                "error",
                "-select_streams",
                "v",
                "-show_entries",
                "stream",
                "-show_format",
                "-show_data_hash",
                "sha256",
                "-show_streams",
                "-of",
                "json",
            ])
            .output()
            .expect("failed to execute process");
        let audio_output = Command::new("ffprobe")
            .args([
                "-i",
                filepath,
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream",
                "-show_format",
                "-show_data_hash",
                "sha256",
                "-show_streams",
                "-of",
                "json",
            ])
            .output()
            .expect("failed to execute process");

        let audio_json_value: Value =
            serde_json::from_str(&String::from_utf8_lossy(&audio_output.stdout)).unwrap();
        let audio_json_str = audio_json_value.to_string();
        let json_value: Value =
            serde_json::from_str(&String::from_utf8_lossy(&video_output.stdout)).unwrap();
        let json_str = json_value.to_string();
        if &json_str.len() >= &1 && &audio_json_str.len() >= &1 {
            let audio_values: Value = audio_json_value;
            let audio_bitrate = audio_values["format"]["bit_rate"].as_str().unwrap_or("0");
            let audio_codec = audio_values["streams"][0]["codec_name"]
                .as_str()
                .unwrap_or("NaN");
            let values: Value = json_value;
            let _width = values["streams"][0]["width"].as_i64().unwrap_or(0);
            let _height = values["streams"][0]["height"].as_i64().unwrap_or(0);
            let filepath = values["format"]["filename"].as_str().unwrap();
            let filename = Path::new(filepath).file_name().unwrap().to_str().unwrap();
            let size = values["format"]["size"].as_str().unwrap_or("0");
            let bitrate = values["format"]["bit_rate"].as_str().unwrap_or("0");
            let duration = values["format"]["duration"].as_str().unwrap_or("0.0");
            let format = values["format"]["format_name"].as_str().unwrap_or("NaN");
            let width = values["streams"][0]["width"].as_i64().unwrap_or(0);
            let height = values["streams"][0]["height"].as_i64().unwrap_or(0);
            let codec = values["streams"][0]["codec_name"].as_str().unwrap_or("NaN");
            let pix_fmt = values["streams"][0]["pix_fmt"].as_str().unwrap_or("NaN");
            let checksum = values["streams"][0]["extradata_hash"]
                .as_str()
                .unwrap_or("NaN");
            let dar = values["streams"][0]["display_aspect_ratio"]
                .as_str()
                .unwrap_or("NaN");
            let sar = values["streams"][0]["sample_aspect_ratio"]
                .as_str()
                .unwrap_or("NaN");

            let mut stmt = conn
                .prepare("UPDATE video_info SET width=?1, height=?2, duration=?3, pixel_format=?4, display_aspect_ratio=?5, sample_aspect_ratio=?6, format=?7, size=?8, folder_size=?9, bitrate=?10, codec=?11, hash=?12 WHERE filename=?13")?;
            stmt.execute(params![
                width,
                height,
                duration,
                pix_fmt,
                dar,
                sar,
                format,
                size,
                new_folder_size,
                bitrate,
                codec,
                checksum,
                filename
            ])?;

            let mut stmt = conn.prepare(
                "UPDATE video_info SET audio_bitrate=?1, audio_codec=?2 WHERE filename=?3",
            )?;
            stmt.execute(params![audio_bitrate, audio_codec, filename])?;
        }
    }

    Ok(())
} */

/* fn create_db_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS video_info (
            id INTEGER PRIMARY KEY,
            filename TEXT NOT NULL,
            filepath TEXT NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            duration TEXT NOT NULL,
            pixel_format TEXT NOT NULL,
            display_aspect_ratio TEXT NOT NULL,
            sample_aspect_ratio TEXT NOT NULL,
            format TEXT NOT NULL,
            size TEXT NOT NULL,
            folder_size INTEGER NOT NULL,
            bitrate TEXT NOT NULL,
            codec TEXT NOT NULL,
            resolution TEXT NOT NULL,
            status TEXT NOT NULL,
            hash TEXT NOT NULL
        )",
        params![],
    )?;
    Ok(())
}
 */
