use rocket::Config;
use shared::*;

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate serde_derive;

// This file is part of the "TransRustica" project
// The goal of this project is to create a database of all the media files in a given folder and it's subfolders and to be able to search through them by resolution, bitrate, codec, etc
// Another goal is to be able to convert all the files in a given folder and it's subfolders to a given format and bitrate
// The bitrate, or crf, is based on the output of "ab-av1" and "vmaf" and the goal is to have a vmaf score of 95 or higher, or the passed in vmaf value
// During the ab-av1 crf-search process, the output of vmaf is used to calculate the crf value that will give a vmaf score of 95 or higher
// During that process, the output of the progress bar should be shown, and used to calculate the ETA
// There should be progress bars for each step of the process, and the ability to stop and resume the process at any time
// The database should be able to be updated with new files and removed files
// The ffmpeg conversion process should be able to be stopped and resumed at any time, and should be able to be run in parallel
// The ffmpeg conversion process should be able to output the important information of the running process, such as ETA, bitrate, etc

use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use rocket::futures::io;
use rocket::tokio::time::Instant;
use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::fs::metadata;
use std::path::Path;
use std::str;
use std::vec;
use std::{string::String, vec::Vec};

use rocket::http::ContentType;
use rocket::response::content::RawHtml;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::PathBuf;

mod routes; // routes.rs

// TODO
// show search progress, spinner, eta, etc
// highlight the (audio?) channel count output with colors
// add audio conversion to aac or opus if audio_codec is not aac or opus or mp3
// fix the progress bar for the ab-av1 crf-search process
// fix the progress bar for the db add process
// print size reduction percentage during the ffmpeg conversion process
// add verbose, information, and debug output
// add a third progress bar for the current file being processed
// ideally have own implementation of vmaf calculation

#[derive(RustEmbed)]
#[folder = "public/"]
struct Asset;

#[get("/")]
fn index() -> Option<RawHtml<Cow<'static, [u8]>>> {
  let asset = Asset::get("index.html")?;
  Some(RawHtml(asset.data))
}

#[get("/dist/<file..>")]
fn dist(file: PathBuf) -> Option<(ContentType, Cow<'static, [u8]>)> {
  let filename = file.display().to_string();
  let asset = Asset::get(&filename)?;
  let content_type = file
    .extension()
    .and_then(OsStr::to_str)
    .and_then(ContentType::from_extension)
    .unwrap_or(ContentType::Bytes);

  Some((content_type, asset.data))
}

#[tokio::main]
async fn main() {
    // Launch the server
    rocket().launch().await.unwrap();
}

#[get("/echo?stream", rank = 1)]
fn echo_stream(ws: rocket_ws::WebSocket) -> rocket_ws::Stream!['static] {
    rocket_ws::Stream! { ws =>
        for await message in ws {
            yield message?;
        }
    }
}

fn rocket() -> rocket::Rocket<rocket::Build> {
    rocket::build()
        .configure(Config {
            address: std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
            port: 8000,
            ..Default::default()
        })
        .mount(
            "/",
            routes![
                //routes::index,
                index,
                dist,
                routes::progress,
                routes::progress_scan,
                routes::start_transcode,
                routes::get_all_from_db_web,
                routes::get_all_from_db_search_web,
                routes::scan_path_web,
                routes::add_to_queue,
                routes::remove_from_queue,
                routes::queue,
                routes::task_ids,
                echo_stream,
                //routes::progress_id_post,
                routes::progress_all,
                routes::progress_id,
            ],
        )
        //.mount("/static", FileServer::from("..\\..\\frontend"))
}

#[tokio::main]
pub async fn main_loop(args_: shared::Args) {
    let main_now = Instant::now();

    // get the arguments from clap and store them in args
    let mut args = args_;

    let encoder = args.encoder.clone();
    let preset_libaom_av1 = args.preset_libaom_av1.clone();
    let preset_hevc_nvenc = args.preset_hevc_nvenc.clone();
    let preset_hevc_qsv = args.preset_hevc_qsv.clone();
    let preset_av1_qsv = args.preset_av1_qsv.clone();
    let preset_libsvtav1 = args.preset_libsvtav1.clone();
    let mut preset_x265 = args.preset_x265.clone();
    let params_x265 = args.params_x265.clone();
    let params_hevc_nvenc = args.params_hevc_nvenc.clone();
    let params_hevc_qsv = args.params_hevc_qsv.clone();
    let params_av1_qsv = args.params_av1_qsv.clone();
    let params_libsvtav1 = args.params_libsvtav1.clone();
    let verbose = args.verbose.clone();
    let task_id = args.task_id.clone();

    if verbose {
        args.verbose = true;
    } else {
        args.verbose = false;
    }

    match encoder.as_str() {
        "libx265" => {
            args.encoder = "libx265".to_string();
        }
        "av1" => {
            args.encoder = "libaom-av1".to_string();
        }
        "libsvtav1" => {
            args.encoder = "libsvtav1".to_string();
        }
        "hevc_nvenc" => {
            args.encoder = "hevc_nvenc".to_string();
        }
        "hevc_qsv" => {
            args.encoder = "hevc_qsv".to_string();
        }
        "av1_qsv" => {
            args.encoder = "av1_qsv".to_string();
        }
        _ => {
            println!("{} is not a valid encoder!", encoder);
            std::process::exit(1);
        }
    }

    // set preset
    if encoder == "av1" {
        preset_x265 = preset_libaom_av1;
    } else if encoder == "hevc_nvenc" {
        preset_x265 = preset_hevc_nvenc;
    } else if encoder == "hevc_qsv" {
        preset_x265 = preset_hevc_qsv;
    } else if encoder == "av1_qsv" {
        preset_x265 = preset_av1_qsv;
    } else if encoder == "libsvtav1" {
        preset_x265 = preset_libsvtav1;
    }

    // set params
    if encoder == "libx265" {
        args.params_x265 = params_x265;
    } else if encoder == "hevc_nvenc" {
        args.params_x265 = params_hevc_nvenc;
    } else if encoder == "hevc_qsv" {
        args.params_x265 = params_hevc_qsv;
    } else if encoder == "av1_qsv" {
        args.params_x265 = params_av1_qsv;
    } else if encoder == "libsvtav1" {
        args.params_x265 = params_libsvtav1;
    }

    // if binary 'ab-av1' is not in the path, exit
    if !std::path::Path::new("ab-av1.exe").exists() {
        println!("Binary 'ab-av1.exe' not found in current path!");
        println!("Searching for ab-av1.exe in system path...");
        // search for binary in system path
        let output = std::process::Command::new("where")
            .arg("ab-av1.exe")
            .output();
        match output {
            Ok(output) => {
                println!(
                    "ab-av1.exe found in: {}",
                    String::from_utf8_lossy(&output.stdout)
                );
            }
            Err(e) => {
                println!("Failed to execute process: {}", e);
            }
        }
    }

    let mut current_file_count = 0;
    let mut total_files: i32;

    let md = metadata(Path::new(&args.inputpath)).unwrap();
    // Check if input is a directory, if yes, check how many video files are in it, and process the ones that are smaller than the given resolution
    if md.is_dir() {
        let mut count;
        let db_count;
        let db_count_added;
        let walk_count: u64 = walk_count(&args.inputpath) as u64;
        let files_bar = ProgressBar::new(walk_count);
        let files_style =
            "[file][{elapsed_precise}] [{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} [analyzed files] eta: {eta:<7}";
        files_bar.set_style(ProgressStyle::default_bar().template(files_style).unwrap());

        let vector_files = walk_files(&args.inputpath);

        let result = add_to_db(vector_files.clone(), files_bar.clone()).unwrap();

        // remove items from db that don't exists anymore, for the given folder and it's subfolders
        remove_from_db_folder(&args.inputpath).unwrap();

        // get the counters from the add_to_db function
        let counters = result.0;

        let to_process = result.1;
        // get the vector of files to process
        let mut vector_files_to_process = to_process.lock().unwrap().clone();

        // count, db_count, db_count_added, db_count_skipped
        count = format!("{:?}", counters[0]).parse::<i32>().unwrap();
        db_count = format!("{:?}", counters[1]).parse::<u64>().unwrap();
        db_count_added = format!("{:?}", counters[2]).parse::<u64>().unwrap();

        if vector_files_to_process.is_empty() {
            let conn = Connection::open("data.db").unwrap();
            let input = args.inputpath.clone();
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM video_info WHERE (status = 'processing' OR status LIKE '%pending%') AND filepath LIKE ?"
                )
                .unwrap();
            let mut rows = stmt.query(&[&format!("%{}%", input)]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                vector_files_to_process.push(row.get(2).unwrap());
            }
        }

        if count == 0 && !vector_files_to_process.is_empty() {
            count = vector_files_to_process.len() as i32;
            current_file_count = db_count - (vector_files_to_process.len() as u64);
        }

        files_bar.finish_and_clear();
        println!(
            "Added {} files to the database ({} already present)",
            db_count_added, db_count
        );

        // print how many files are to be processed
        println!("Processing {} files...", count);

        // count the total number of frames to be processed
        let mut total_frames: u64 = 0;
        let frame_count_progress_bar = ProgressBar::new(vector_files_to_process.len() as u64);
        let frame_count_progress_bar_style = ProgressStyle::default_bar().template(
            "[frmcnt][{elapsed_precise}][{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]"
        );
        frame_count_progress_bar.set_style(frame_count_progress_bar_style.unwrap());

        for file in vector_files_to_process.clone() {
            let frame_count = get_framecount_tag(&file).unwrap_or_else(|_| {
                get_framecount_metadata(&file).unwrap_or_else(|_| {
                    get_framecount_ffmpeg(&file)
                        .unwrap_or_else(|_| get_framecount(&file).unwrap_or_else(|_| 0.0))
                })
            });
            total_frames = total_frames + (frame_count as u64);
            frame_count_progress_bar.inc(1);
        }

        frame_count_progress_bar.finish();

        // Print the total number of frames to be processed, within a total number of files
        println!(
            "Total number of frames to be processed: {} in {} files",
            total_frames, count
        );

        // create a vector of all the files to process, with their frame count
        let mut vector_files_to_process_frame_count: Vec<(String, u64)> = Vec::new();
        // add entry of 0
        vector_files_to_process_frame_count.push(("".to_string(), 0));
        for file in vector_files_to_process.clone() {
            let frame_count = get_framecount_tag(&file).unwrap_or_else(|_| {
                get_framecount_metadata(&file).unwrap_or_else(|_| {
                    get_framecount_ffmpeg(&file)
                        .unwrap_or_else(|_| get_framecount(&file).unwrap_or_else(|_| 0.0))
                })
            });
            vector_files_to_process_frame_count.push((file, frame_count as u64));
        }

        // setup progress bar and show count for each file being processed
        let total_style = ProgressStyle::default_bar().template(
            "[frames][{elapsed_precise}] [{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]"
        );
        let transcode_style = ProgressStyle::default_bar().template(
            "[ffmpeg][{elapsed_precise}] [{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]"
        );
        let info_style = ProgressStyle::default_bar().template("[info][{msg}]");
        let codec_style = ProgressStyle::default_bar().template("[codec][{msg}]");
        let file_bar_style = ProgressStyle::default_bar().template("[file]{msg}");
        let vmaf_bar_style =
            ProgressStyle::default_bar().template("[vmaf][{elapsed_precise}][{msg}]");
        let m = MultiProgress::new();
        let file_bar = m.add(ProgressBar::new(0));
        file_bar.set_style(file_bar_style.unwrap());
        let total_bar = m.add(ProgressBar::new(total_frames));
        total_bar.set_style(total_style.unwrap());
        let transcode_bar = m.add(ProgressBar::new(0));
        transcode_bar.set_style(transcode_style.unwrap());
        let info_bar = m.add(ProgressBar::new(0));
        info_bar.set_style(info_style.unwrap());
        let codec_bar = m.add(ProgressBar::new(0));
        codec_bar.set_style(codec_style.unwrap());
        let vmaf_bar = m.add(ProgressBar::new(0));
        vmaf_bar.set_style(vmaf_bar_style.unwrap());

        for file in vector_files_to_process.clone() {
            current_file_count = current_file_count + 1;
            total_files = vector_files_to_process.len() as i32;

            // print the current file being processed
            println!("Processing file {} of {}...", current_file_count, count);

            /*             // Check status of file in database
            let conn = Connection::open("data.db").unwrap();
            let mut stmt = conn
                .prepare("SELECT * FROM video_info WHERE filepath = ?")
                .unwrap();
            let mut rows = stmt.query(&[&file]).unwrap();
            let mut status = String::new();
            while let Some(row) = rows.next().unwrap() {
                status = row.get(14).unwrap();
            } */

            args.inputpath = file.clone();
            args.inputpath = absolute_path(file.clone());

            /*             // update status in sqlite database 'data.db' to processing for this file where filepaths match the current file
            let conn = Connection::open("data.db").unwrap();
            conn.execute(
                "UPDATE video_info SET status = 'processing' WHERE filepath = ?",
                &[&args.inputpath],
            )
            .unwrap(); */

            let _vmaf = &args.vmaf;
            let _encoder = &args.encoder;
            let _params_x265 = &args.params_x265;
            let _pix_fmt = &args.pix_fmt;
            let _preset_x265 = &preset_x265;
            let _max_crf = &args.max_crf;
            let _sample_every = &args.sample_every;
            let _vmaf_threads = &args.vmaf_threads;
            let _verbose = &args.verbose;
            let _mode = &args.mode;

            if _mode == "default" {
                println!("Running default ab-av1...");

                // TEMP TODO place this in a function
                // For each audio track in audio_details, check if it's codec is aac or opus, if not, convert it to opus wit the same channel count and channel layout
                // Only process if codex is: flac, ac3, eac3, dts or truehd
                // Use -c:a libopus -b:a 128k
                // 128 kbps is recommended for quality stereo music, for channel_count == 2.
                // 256 kbps for 5.1 surround, for channel_count > 2 but less than 6.
                // 450 kbps for 7.1 surround sound, for channel_count > 6.

                let audio_details = get_audio_details(&file).unwrap();
                let video_details = get_video_details(&file).unwrap();
                let audio_details_clone = audio_details.clone();
                let mut _audio_arg: String = "".to_string();
                let mut vec_audio_args: Vec<(usize, String, String)> = Vec::new();
                let mut vec_video_args: Vec<(usize, String, String, String)> = Vec::new();
                let mut original_audio_codec: String = "".to_string();

                let mut i = 0;
                for (audio_codec, mut audio_channel_count) in audio_details {
                    if &audio_codec != "aac" && audio_codec != "opus" && audio_codec != "mp3" {
                        if audio_channel_count == "2" {
                            _audio_arg =
                                format!("-c:a:{} libopus -b:a 128k -ac {}", i, audio_channel_count);
                            original_audio_codec = audio_details_clone[i].0.clone();
                            vec_audio_args.push((
                                i,
                                _audio_arg.clone(),
                                original_audio_codec.clone(),
                            ));
                        } else if audio_channel_count == "6" {
                            audio_channel_count = "5.1".to_string();
                            _audio_arg = format!(
                                //"-c:a:{} libopus -af channelmap=channel_layout={} -b:a 256k",
                                "-c:a:{} libopus -channel_layout:a:{} {} -b:a 256k",
                                //i, audio_channel_count
                                i,
                                i,
                                audio_channel_count
                            );
                            original_audio_codec = audio_details_clone[i].0.clone();
                            vec_audio_args.push((
                                i,
                                _audio_arg.clone(),
                                original_audio_codec.clone(),
                            ));
                        } else if audio_channel_count == "8" {
                            audio_channel_count = "7.1".to_string();
                            _audio_arg = format!(
                                //"-c:a:{} libopus -af channelmap=channel_layout={} -b:a 450k",
                                //i, audio_channel_count
                                "-c:a:{} libopus -channel_layout:a:{} {} -b:a 450k",
                                i, i, audio_channel_count
                            );
                            original_audio_codec = audio_details_clone[i].0.clone();
                            vec_audio_args.push((
                                i,
                                _audio_arg.clone(),
                                original_audio_codec.clone(),
                            ));
                        }
                    } else {
                        _audio_arg = "".to_string();
                        original_audio_codec = audio_details_clone[i].0.clone();
                        vec_audio_args.push((i, _audio_arg.clone(), original_audio_codec.clone()));
                    }
                    i = i + 1;
                }

                let mut j = 0;
                for (video_codec, video_width, video_height) in video_details {
                    let video_arg = format!("{}", video_codec.clone());
                    vec_video_args.push((j, video_arg.clone(), video_width, video_height));
                    j = j + 1;
                }

                // // if (size is over 600MB or) bitrate is over 3MB/s then add to db with status pending_video, otherwise add to db with status skipped
                // // also if audio_codec is not aac or opus then add to db with status pending_audio, otherwise add to db with status skipped
                // // if both video and audio are pending then add to db with status pending_all
                let mut status = "pending_video";
                let bitrate = get_bitrate(&file);
                // Convert bitrate to f64
                let bitrate = bitrate.parse::<f64>().unwrap();
                let audio_codec = get_audio_details(&file).unwrap();
                // get the first audio codec
                let audio_codec = audio_codec[0].0.clone();
                let _transcode_info = "".to_string();

                if bitrate < 3000.0 {
                    status = "skipped";
                }
                if bitrate > 3000.0 {
                    status = "pending_video";
                }
                if audio_codec != "aac" && audio_codec != "opus" && audio_codec != "mp3" {
                    status = "pending_audio";
                }
                if bitrate > 3000.0
                    && audio_codec != "aac"
                    && audio_codec != "opus"
                    && audio_codec != "mp3"
                {
                    status = "pending_all";
                }

                /*                 // Get the status of the file from the database
                let conn = Connection::open("data.db").unwrap();
                let mut stmt = conn
                    .prepare("SELECT * FROM video_info WHERE filepath = ?")
                    .unwrap();
                let mut rows = stmt.query(&[&file]).unwrap();
                let mut status_db = String::new();
                while let Some(row) = rows.next().unwrap() {
                    status_db = row.get(14).unwrap();
                }

                let status: &str = &status_db; */

                // Match status, if status is pending_video, transcode_info = "video", if status is pending_audio, transcode_info = "audio", if status is pending_all, transcode_info = "all"
                let transcode_info = match status {
                    "pending_video" => "video",
                    "pending_audio" => "audio",
                    "pending_all" => "all",
                    _ => "",
                };

                if status == "pending_audio".to_string() {
                    // set_output_folder function
                    let final_output = set_output_folder_filename_audio(&file, &args.outputpath);

                    run_ffmpeg_transcode_audio(
                        &file,
                        &final_output,
                        &file_bar,
                        &transcode_bar,
                        &total_bar,
                        &info_bar,
                        &codec_bar,
                        &total_files,
                        &current_file_count,
                        &vector_files_to_process_frame_count,
                        &original_audio_codec,
                        &transcode_info,
                        &vec_audio_args,
                        &vec_video_args,
                        &task_id,
                    );
                } else {
                    // run ab-av1.exe to find the best crf for the file
                    let crf_search_result = run_ab_av1_crf_search(
                        &file,
                        &encoder,
                        &preset_x265,
                        _pix_fmt,
                        *_vmaf,
                        _max_crf,
                        _sample_every,
                        _vmaf_threads,
                        *_verbose,
                        &task_id,
                        &current_file_count,
                        &total_files,
                    );

                    let crf_search_result_unwrap = crf_search_result.unwrap();
                    let output_final = crf_search_result_unwrap.clone();

                    // set_output_folder function
                    let final_output = set_output_folder_filename(
                        &file,
                        &encoder,
                        &output_final.1,
                        &output_final.0,
                        &args.output_folder,
                    );

                    // run ffmpeg.exe to encode the file
                    run_ffmpeg_transcode(
                        &file,
                        &encoder,
                        &_params_x265,
                        &preset_x265,
                        _pix_fmt,
                        &final_output,
                        // use the result from run_ab_av1_crf_search function
                        &output_final.0.to_string(),
                        &file_bar,
                        &transcode_bar,
                        &total_bar,
                        &info_bar,
                        &codec_bar,
                        &total_files,
                        &current_file_count,
                        &vector_files_to_process_frame_count,
                        &output_final.1,
                        &original_audio_codec,
                        &transcode_info,
                        &vec_audio_args,
                        &vec_video_args,
                        &task_id,
                    );
                }
            } else if _mode == "chunked" {
                println!("Running chunked...");
                let scene_changes = run_ffmpeg_scene_change(&file, &args);
                let scene_changes_clone = scene_changes.unwrap().clone();
                let scene_changes_clone2 = scene_changes_clone.clone();

                // for each scene in scene_changes run get_scene_size
                // Create a vector of (scene_index, scene_size)
                // Possibly add this to the vector of scene_changes
                let scene_changes_clone = scene_changes_clone;
                let mut scene_sizes: Vec<(i32, i32)> = Vec::new();
                let mut scenes: Vec<(f32, f32)> = Vec::new();
                {
                    let scene_changes_locked = scene_changes_clone;
                    for window in scene_changes_locked.windows(2) {
                        scenes.push((window[0], window[1]));
                    }
                }

                // Create a progress bar
                let progress_bar = ProgressBar::new(scenes.len() as u64);
                let progress_bar_style =
                    "[scs][{elapsed_precise}][{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]";
                progress_bar.set_style(
                    ProgressStyle::default_bar()
                        .template(progress_bar_style)
                        .unwrap(),
                );

                let mut scene_index = 0;
                for (scene_start, scene_end) in &scenes {
                    let ss = format_timecode(&scene_start);
                    let to = format_timecode(&scene_end);
                    let scene_size = get_scene_size(&file, &ss, &to);

                    scene_sizes.push((scene_index, scene_size.unwrap()));
                    scene_index += 1;
                    progress_bar.inc(1);
                }

                let scene_changes_vec = scene_changes_clone2;

                let _scene_changes = run_ffmpeg_extract_scene_changes_pipe_vmaf_target_threaded(
                    &file,
                    &scene_changes_vec,
                    &scene_sizes,
                    &args,
                    &get_fps_f32(&file),
                );

                // TODO make conversion inside function, directly after calculation
                // It should probably have it's own function and progress bar
                let scene_changes = _scene_changes.unwrap();

                let vmaf_f32 = args.vmaf as f32;
                let mut closest_scores: HashMap<i32, (i32, f32, f32)> = HashMap::new();
                for (scene_index, crf, vmaf_score) in &scene_changes {
                    if let Some((_, _, stored_vmaf)) = closest_scores.get(scene_index) {
                        let current_diff = (vmaf_score - &vmaf_f32).abs();
                        let stored_diff = (stored_vmaf - &vmaf_f32).abs();

                        if current_diff < stored_diff {
                            closest_scores.insert(*scene_index, (*scene_index, *crf, *vmaf_score));
                        }
                    } else {
                        closest_scores.insert(*scene_index, (*scene_index, *crf, *vmaf_score));
                    }
                }

                // Convert hashmap back to a vector if needed
                let mut closest_scores_vec: Vec<(i32, f32, f32)> =
                    closest_scores.values().cloned().collect();
                // Sort the vector by scene_index
                closest_scores_vec.sort_by(|a, b| a.0.cmp(&b.0));
                // Print all _scene_changes
                for (scene_index, crf, vmaf_score) in &closest_scores_vec {
                    println!("{} {} {}", scene_index, crf, vmaf_score);
                }
            } else {
                println!("{} is not a valid mode!", _mode);
                std::process::exit(1);
            }
        }

        let elapsed = main_now.elapsed();
        let seconds = elapsed.as_secs() % 60;
        let minutes = (elapsed.as_secs() / 60) % 60;
        let hours = elapsed.as_secs() / 60 / 60;
        println!(
            "done {} files in {}h:{}m:{}s",
            count, hours, minutes, seconds
        );
    } else if md.is_file() {
        // do find map_metadata_audio and map_metadata_subtitle using run_ffmpeg_map_metadata function
        // run ab-av1.exe to find the best crf for the file
        // set_output_folder function
        // run ffmpeg.exe to encode the file

        let file = args.inputpath.clone();
        let _vmaf = &args.vmaf;
        let _encoder = &args.encoder;
        let _params_x265 = &args.params_x265;
        let _pix_fmt = &args.pix_fmt;
        let _preset_x265 = &preset_x265;
        let _max_crf = &args.max_crf;
        let _sample_every = &args.sample_every;
        let _vmaf_threads = &args.vmaf_threads;
        let _verbose = &args.verbose;
        let _total_files = 1;
        let _mode = &args.mode;

        /*         // Check status of file in database
        let conn = Connection::open("data.db").unwrap();
        let mut stmt = conn
            .prepare("SELECT * FROM video_info WHERE filepath = ?")
            .unwrap();
        let mut rows = stmt.query(&[&file]).unwrap();
        let mut status = String::new();
        while let Some(row) = rows.next().unwrap() {
            status = row.get(15).unwrap();
        } */

        // TEMP TODO place this in a function
        // For each audio track in audio_details, check if it's codec is aac or opus, if not, convert it to opus wit the same channel count and channel layout
        // Only process if codex is: flac, ac3, eac3, dts or truehd
        // Use -c:a libopus -b:a 128k
        // 128 kbps is recommended for quality stereo music, for channel_count == 2.
        // 256 kbps for 5.1 surround, for channel_count > 2 but less than 6.
        // 450 kbps for 7.1 surround sound, for channel_count > 6.

        let audio_details = get_audio_details(&file).unwrap();
        let video_details = get_video_details(&file).unwrap();
        let audio_details_clone = audio_details.clone();
        let mut _audio_arg: String = "".to_string();
        let mut vec_audio_args: Vec<(usize, String, String)> = Vec::new();
        let mut vec_video_args: Vec<(usize, String, String, String)> = Vec::new();
        let mut original_audio_codec: String = "".to_string();

        let mut i = 0;
        for (audio_codec, mut audio_channel_count) in audio_details {
            if &audio_codec != "aac" && audio_codec != "opus" && audio_codec != "mp3" {
                if audio_channel_count == "2" {
                    _audio_arg =
                        format!("-c:a:{} libopus -b:a 128k -ac {}", i, audio_channel_count);
                    original_audio_codec = audio_details_clone[i].0.clone();
                    vec_audio_args.push((i, _audio_arg.clone(), original_audio_codec.clone()));
                } else if audio_channel_count == "6" {
                    audio_channel_count = "5.1".to_string();
                    _audio_arg = format!(
                        //"-c:a:{} libopus -af channelmap=channel_layout={} -b:a 256k",
                        //i, audio_channel_count
                        "-c:a:{} libopus -channel_layout:a:{} {} -b:a 256k",
                        i, i, audio_channel_count
                    );
                    original_audio_codec = audio_details_clone[i].0.clone();
                    vec_audio_args.push((i, _audio_arg.clone(), original_audio_codec.clone()));
                } else if audio_channel_count == "8" {
                    audio_channel_count = "7.1".to_string();
                    _audio_arg = format!(
                        //"-c:a:{} libopus -af channelmap=channel_layout={} -b:a 450k",
                        //i, audio_channel_count
                        "-c:a:{} libopus -channel_layout:a:{} {} -b:a 450k",
                        i, i, audio_channel_count
                    );
                    original_audio_codec = audio_details_clone[i].0.clone();
                    vec_audio_args.push((i, _audio_arg.clone(), original_audio_codec.clone()));
                }
            } else {
                _audio_arg = "".to_string();
                original_audio_codec = audio_details_clone[i].0.clone();
                vec_audio_args.push((i, _audio_arg.clone(), original_audio_codec.clone()));
            }
            i = i + 1;
        }

        let mut j = 0;
        for (video_codec, video_width, video_height) in video_details {
            let video_arg = format!("{}", video_codec.clone());
            vec_video_args.push((j, video_arg.clone(), video_width, video_height));
            j = j + 1;
        }

        if _mode == "default" {
            println!("Running default ab-av1...");

            // Get the number of frames in the file
            let total_frames = get_framecount_tag(&file).unwrap_or_else(|_| {
                get_framecount_metadata(&file).unwrap_or_else(|_| {
                    get_framecount_ffmpeg(&file)
                        .unwrap_or_else(|_| get_framecount(&file).unwrap_or_else(|_| 0.0))
                })
            }) as u64;

            // setup progress bar and show count for each file being processed
            let total_style = ProgressStyle::default_bar().template(
                "[frames][{elapsed_precise}] [{wide_bar:.green/white}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]"
            );
            let transcode_style = ProgressStyle::default_bar().template(
                "[ffmpeg][{elapsed_precise}] [{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]"
            );
            let info_style = ProgressStyle::default_bar().template("[info][{msg}]");
            let codec_style = ProgressStyle::default_bar().template("[codec][{msg}]");
            let file_bar_style = ProgressStyle::default_bar().template("[file]{msg}");
            let vmaf_bar_style =
                ProgressStyle::default_bar().template("[vmaf][{elapsed_precise}][{msg}]");
            let m = MultiProgress::new();
            let file_bar = m.add(ProgressBar::new(0));
            file_bar.set_style(file_bar_style.unwrap());
            let total_bar = m.add(ProgressBar::new(total_frames));
            total_bar.set_style(total_style.unwrap());
            let transcode_bar = m.add(ProgressBar::new(0));
            transcode_bar.set_style(transcode_style.unwrap());
            let info_bar = m.add(ProgressBar::new(0));
            info_bar.set_style(info_style.unwrap());
            let codec_bar = m.add(ProgressBar::new(0));
            codec_bar.set_style(codec_style.unwrap());
            let vmaf_bar = m.add(ProgressBar::new(0));
            vmaf_bar.set_style(vmaf_bar_style.unwrap());

            // // if (size is over 600MB or) bitrate is over 3MB/s then add to db with status pending_video, otherwise add to db with status skipped
            // // also if audio_codec is not aac or opus then add to db with status pending_audio, otherwise add to db with status skipped
            // // if both video and audio are pending then add to db with status pending_all
            let mut status = "pending_video";
            let bitrate = get_bitrate(&file);
            // Convert bitrate to f64
            let bitrate = bitrate.parse::<f64>().unwrap();
            let audio_codec = get_audio_details(&file).unwrap();
            // get the first audio codec
            let audio_codec = audio_codec[0].0.clone();
            let _transcode_info = "".to_string();

            if bitrate < 3000.0 {
                status = "skipped";
            }
            if bitrate > 3000.0 {
                status = "pending_video";
            }
            if audio_codec != "aac" && audio_codec != "opus" && audio_codec != "mp3" {
                status = "pending_audio";
            }
            if bitrate > 3000.0
                && audio_codec != "aac"
                && audio_codec != "opus"
                && audio_codec != "mp3"
            {
                status = "pending_all";
            }

            /*             // Get the status of the file from the database
            let conn = Connection::open("data.db").unwrap();
            let mut stmt = conn
                .prepare("SELECT * FROM video_info WHERE filepath = ?")
                .unwrap();
            let mut rows = stmt.query(&[&file]).unwrap();
            let mut status_db = String::new();
            while let Some(row) = rows.next().unwrap() {
                status_db = row.get(14).unwrap();
            }

            let status: &str = &status_db; */

            // Match status, if status is pending_video, transcode_info = "video", if status is pending_audio, transcode_info = "audio", if status is pending_all, transcode_info = "all"
            let transcode_info = match status {
                "pending_video" => "video",
                "pending_audio" => "audio",
                "pending_all" => "all",
                _ => "",
            };

            if status == "pending_audio".to_string() {
                // set_output_folder function
                let final_output = set_output_folder_filename_audio(&file, &args.outputpath);

                run_ffmpeg_transcode_audio(
                    &file,
                    &final_output,
                    &file_bar,
                    &transcode_bar,
                    &total_bar,
                    &info_bar,
                    &codec_bar,
                    &_total_files,
                    &current_file_count,
                    &vec![], // empty vector
                    &original_audio_codec,
                    &transcode_info,
                    &vec_audio_args,
                    &vec_video_args,
                    &task_id,
                );
            } else {
                let current_file_count = 1;
                let total_files = 1;

                // run ab-av1.exe to find the best crf for the file
                let crf_search_result = run_ab_av1_crf_search(
                    &file,
                    &encoder,
                    &preset_x265,
                    _pix_fmt,
                    *_vmaf,
                    _max_crf,
                    _sample_every,
                    _vmaf_threads,
                    *_verbose,
                    &args.task_id,
                    &current_file_count,
                    &total_files,
                );

                let crf_search_result_unwrap = crf_search_result.unwrap();
                let output_final = crf_search_result_unwrap.clone();

                // set_output_folder function
                let final_output = set_output_folder_filename(
                    &file,
                    &encoder,
                    &output_final.1,
                    &output_final.0,
                    &args.output_folder,
                );

                // TEMP Print final_output
                println!("final_output: {}", final_output);

                // run ffmpeg.exe to encode the file
                run_ffmpeg_transcode(
                    &file,
                    &encoder,
                    &_params_x265,
                    &preset_x265,
                    _pix_fmt,
                    &final_output,
                    // use the result from run_ab_av1_crf_search function
                    &output_final.0.to_string(),
                    &file_bar,
                    &transcode_bar,
                    &total_bar,
                    &info_bar,
                    &codec_bar,
                    &_total_files,
                    &current_file_count,
                    &vec![], // empty vector
                    &output_final.1,
                    &original_audio_codec,
                    &transcode_info,
                    &vec_audio_args,
                    &vec_video_args,
                    &task_id,
                );
            }
        } else if _mode == "chunked" {
            println!("Running chunked...");
            let scene_changes = shared::run_ffmpeg_scene_change(&file, &args);
            let scene_changes_clone = scene_changes.unwrap().clone();
            let scene_changes_clone2 = scene_changes_clone.clone();

            // for each scene in scene_changes run get_scene_size
            // Create a vector of (scene_index, scene_size)
            // Possibly add this to the vector of scene_changes
            let scene_changes_clone = scene_changes_clone;
            let mut scene_sizes: Vec<(i32, i32)> = Vec::new();
            let mut scenes: Vec<(f32, f32)> = Vec::new();
            {
                let scene_changes_locked = scene_changes_clone;
                for window in scene_changes_locked.windows(2) {
                    scenes.push((window[0], window[1]));
                }
            }

            // Create a progress bar
            let progress_bar = ProgressBar::new(scenes.len() as u64);
            let progress_bar_style =
                "[scs][{elapsed_precise}][{wide_bar:.cyan/blue}] {percent:3} {pos:>7}/{len:7} [ETA: {eta:<3}]";
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template(progress_bar_style)
                    .unwrap(),
            );

            let mut scene_index = 0;
            for (scene_start, scene_end) in &scenes {
                let ss = format_timecode(&scene_start);
                let to = format_timecode(&scene_end);
                let scene_size = shared::get_scene_size(&file, &ss, &to);

                scene_sizes.push((scene_index, scene_size.unwrap()));
                scene_index += 1;
                progress_bar.inc(1);
            }

            let scene_changes_vec = scene_changes_clone2;

            let _scene_changes = run_ffmpeg_extract_scene_changes_pipe_vmaf_target_threaded(
                &file,
                &scene_changes_vec,
                &scene_sizes,
                &args,
                &get_fps_f32(&file),
            );

            // TODO make conversion inside function, directly after calculation
            // It should probably have it's own function and progress bar
            let scene_changes = _scene_changes.unwrap();

            let vmaf_f32 = args.vmaf as f32;
            let mut closest_scores: HashMap<i32, (i32, f32, f32)> = HashMap::new();
            for (scene_index, crf, vmaf_score) in &scene_changes {
                if let Some((_, _, stored_vmaf)) = closest_scores.get(scene_index) {
                    let current_diff = (vmaf_score - &vmaf_f32).abs();
                    let stored_diff = (stored_vmaf - &vmaf_f32).abs();

                    if current_diff < stored_diff {
                        closest_scores.insert(*scene_index, (*scene_index, *crf, *vmaf_score));
                    }
                } else {
                    closest_scores.insert(*scene_index, (*scene_index, *crf, *vmaf_score));
                }
            }

            // Convert hashmap back to a vector if needed
            let mut closest_scores_vec: Vec<(i32, f32, f32)> =
                closest_scores.values().cloned().collect();
            // Sort the vector by scene_index
            closest_scores_vec.sort_by(|a, b| a.0.cmp(&b.0));
            // Print all _scene_changes
            for (scene_index, crf, vmaf_score) in &closest_scores_vec {
                println!("{} {} {}", scene_index, crf, vmaf_score);
            }
        } else {
            println!("{} is not a valid mode!", _mode);
            std::process::exit(1);
        }
    }

    let elapsed = main_now.elapsed();
    let seconds = elapsed.as_secs() % 60;
    let minutes = (elapsed.as_secs() / 60) % 60;
    let hours = elapsed.as_secs() / 60 / 60;
    println!(
        "done {} files in {}h:{}m:{}s",
        current_file_count, hours, minutes, seconds
    );
}

pub fn run_from_web(
    web_uuid: &str,
    web_input_path: &str,
    web_output_path: &str,
    web_encoder: &str,
    web_vmaf: &str,
    web_vmaf_threads: &str,
) -> Result<(), io::Error> {
    // convert web_vmaf to i32
    let web_vmaf = web_vmaf.parse::<i32>().unwrap();

    // set up args
    let args = Args {
        inputpath: web_input_path.to_string(),
        outputpath: web_output_path.to_string(),
        output_folder: web_output_path.to_string(),
        encoder: web_encoder.to_string(),
        vmaf: web_vmaf,
        vmaf_threads: web_vmaf_threads.to_string(),
        vmaf_pool: "mean".to_string(),
        vmaf_subsample: "1".to_string(),
        mode: "default".to_string(),
        verbose: false,
        pix_fmt: "yuv420p10le".to_string(),
        max_crf: "28".to_string(),
        sample_every: "3m".to_string(),
        params_ab_av1: "x265-params=limit-sao,bframes=8,psy-rd=1,aq-mode=3".to_string(),
        params_x265: "-x265-params limit-sao:bframes=8:psy-rd=1:aq-mode=3".to_string(),
        preset_x265: "veryslow".to_string(),
        preset_av1: "8".to_string(),
        preset_hevc_nvenc: "p7".to_string(),
        params_hevc_nvenc: "-rc:v vbr_hq -rc-lookahead 100 -b_ref_mode each -tune hq".to_string(),
        preset_hevc_qsv: "veryslow".to_string(),
        params_hevc_qsv: "-init_hw_device qsv=intel,child_device=0 -b_strategy 1".to_string(),
        preset_av1_qsv: "8".to_string(),
        params_av1_qsv: "-init_hw_device qsv=intel,child_device=0 -b_strategy 1".to_string(),
        preset_libsvtav1: "8".to_string(),
        params_libsvtav1: "-enc-mode 1 -enc-params 1".to_string(),
        preset_libaom_av1: "8".to_string(),
        params_libaom_av1:
            "--cpu-used=3 --end-usage=q --cq-level=30 --tile-columns=2 --tile-rows=2".to_string(),
        scene_split_min: 2.0,
        task_id: web_uuid.to_string(),
    };

    main_loop(args);

    Ok(())
}
