use crate::run_from_web;
use base64::{engine::general_purpose, Engine};
use indicatif::ProgressBar;
use lazy_static::lazy_static;
use rocket::serde::json::Json;
use serde_json::{json, Value};
use shared::*;
use std::{path::PathBuf, sync::Mutex, thread};

lazy_static! {
    static ref TASK_IDS: Mutex<Vec<String>> = Mutex::new(vec![]);
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskDetails {
    input_path: String,
    output_path: String,
    encoder: String,
    preset: String,
    vmaf_target: String,
    vmaf_threads: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueueItem {
    input_path: String,
    output_path: String,
    encoder: String,
    preset: String,
    vmaf_target: String,
    vmaf_threads: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueueInput {
    input_path: String,
    output_path: String,
    encoder: String,
    preset: String,
    vmaf_target: String,
    vmaf_threads: String,
}

/* #[get("/")]
pub async fn index() -> NamedFile {
    NamedFile::open("..\\..\\frontend/index.html")
        .await
        .unwrap()
} */

#[get("/progress")]
pub fn progress() -> Json<Progress> {
    // Get the progress of the task
    let progress = get_progress_web();

    // Return the progress as JSON
    Json(progress)
}

#[get("/progress/<id>")]
pub fn progress_id(id: String) -> Json<Progress> {
    // Get the progress of the task
    let progress = get_progress_web_id(id);

    // Return the progress as JSON
    Json(progress)
}

/* #[post("/progress/<id>", format = "json", data = "<progress>")]
pub fn progress_id_post(id: String, progress: Json<Progress>) -> Json<Progress> {
    // Get the progress of the task
    let progress = get_progress_web_id(id);

    // Return the progress as JSON
    Json(progress)
} */

// Function to get the progress of all the tasks
#[get("/progress_all")]
pub fn progress_all() -> Json<Vec<Progress>> {
    // Get the progress of the task
    let progress = get_progress_web();

    // Create a vector to hold all the progresses
    let mut progresses = Vec::new();

    progresses.push(progress);

    // TEMP Print the progresses
    //println!("GET:{:?}", progresses);

    // Return the progress as JSON
    Json(progresses)
}

/* #[post("/progress_all", format = "json", data = "<progress>")]
pub fn progress_id_post(progress: Json<Progress>) -> Json<Vec<Progress>> {
    // Create a vector to hold all the progresses
    let mut progresses = Vec::new();

    // Get the progress of the task
    progresses.push(progress.into_inner());

    // TEMP Print the progresses
    //println!("POST:{:?}", progresses);

    // Collect the progress, and return them as JSON
    Json(progresses)
} */

#[get("/progress_scan")]
pub fn progress_scan() -> Json<ProgressScan> {
    // Get the progress of the task
    let progress_scan = get_progress_scan_web();

    // Return the progress as JSON
    Json(progress_scan)
}

#[get("/all")]
pub fn get_all_from_db_web() -> Json<Vec<Value>> {
    // Get all the tasks from the database
    let tasks = get_all_from_db().unwrap();

    // Create a vector to hold all the tasks as JSON objects
    let mut json_objects = Vec::new();

    // Iterate over the tasks
    for task in tasks {
        let id = task.0.clone();
        let filename = task.1.clone();
        let filepath = task.2.clone();
        let width = task.3.clone();
        let height = task.4.clone();
        let duration = task.5.clone();
        let pixel_format = task.6.clone();
        let display_aspect_ratio = task.7.clone();
        let sample_aspect_ratio = task.8.clone();
        let format = task.9.clone();
        let size = task.10.clone();
        let folder_size = task.11.clone();
        let bitrate = task.12.clone();
        let codec = task.13.clone();
        let status = task.14.clone();
        let audio_codec = task.15.clone();
        let audio_bitrate = task.16.clone();
        let hash = task.17.clone();

        // Serialize the values into a JSON object
        let json_object = json!({
            "id": id,
            "filename": filename,
            "filepath": filepath,
            "width": width,
            "height": height,
            "duration": duration,
            "pixel_format": pixel_format,
            "display_aspect_ratio": display_aspect_ratio,
            "sample_aspect_ratio": sample_aspect_ratio,
            "format": format,
            "size": size,
            "folder_size": folder_size,
            "bitrate": bitrate,
            "codec": codec,
            "status": status,
            "audio_codec": audio_codec,
            "audio_bitrate": audio_bitrate,
            "hash": hash
        });

        // Add the JSON object to the vector
        json_objects.push(json_object);
    }

    // Return the vector as JSON
    Json(json_objects)
}

#[get("/search/<search>")]
pub fn get_all_from_db_search_web(search: String) -> Json<Vec<Value>> {
    // Get all the tasks from the database
    let tasks = get_all_from_db_search(&search).unwrap();

    // Create a vector to hold all the tasks as JSON objects
    let mut json_objects = Vec::new();

    // Iterate over the tasks
    for task in tasks {
        let id = task.0.clone();
        let filename = task.1.clone();
        let filepath = task.2.clone();
        let width = task.3.clone();
        let height = task.4.clone();
        let duration = task.5.clone();
        let pixel_format = task.6.clone();
        let display_aspect_ratio = task.7.clone();
        let sample_aspect_ratio = task.8.clone();
        let format = task.9.clone();
        let size = task.10.clone();
        let folder_size = task.11.clone();
        let bitrate = task.12.clone();
        let codec = task.13.clone();
        let status = task.14.clone();
        let audio_codec = task.15.clone();
        let audio_bitrate = task.16.clone();
        let hash = task.17.clone();

        // Serialize the values into a JSON object
        let json_object = json!({
            "id": id,
            "filename": filename,
            "filepath": filepath,
            "width": width,
            "height": height,
            "duration": duration,
            "pixel_format": pixel_format,
            "display_aspect_ratio": display_aspect_ratio,
            "sample_aspect_ratio": sample_aspect_ratio,
            "format": format,
            "size": size,
            "folder_size": folder_size,
            "bitrate": bitrate,
            "codec": codec,
            "status": status,
            "audio_codec": audio_codec,
            "audio_bitrate": audio_bitrate,
            "hash": hash
        });

        // Add the JSON object to the vector
        json_objects.push(json_object);
    }

    // Return the vector as JSON
    Json(json_objects)
}

#[post("/scan/<base64>")]
pub fn scan_path_web(base64: String) -> Json<Value> {
    // Decode the base64 string
    let path = general_purpose::STANDARD.decode(base64.as_bytes()).unwrap();
    let path = String::from_utf8(path).unwrap();

    // Validate the path is a directory
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Json(json!({"status": "error", "message": "Path is not a directory"}));
    }

    let number_of_files = walk_count(&path.to_str().unwrap().to_string());
    let files_vec = walk_files(&path.to_str().unwrap().to_string());
    let bar = ProgressBar::new(number_of_files as u64);

    // Add the task to the database
    let _task = add_to_db(files_vec, bar);

    // Return the task as JSON
    Json(json!({"status": "success"}))
}

#[post("/start_transcode", data = "<task_details>")]
pub fn start_transcode(task_details: Json<TaskDetails>) -> Json<Vec<String>> {
    // Generate a UUID for the task
    let id = uuid::Uuid::new_v4().to_string();
    let id_clone = id.clone();

    thread::spawn(move || {
        // Start the task in a new thread
        let _task = run_from_web(
            &id,
            &task_details.input_path,
            &task_details.output_path,
            &task_details.encoder,
            &task_details.vmaf_target,
            &task_details.vmaf_threads,
        );
    });

    let mut task_ids = TASK_IDS.lock().unwrap();

    task_ids.push(id_clone);

    Json(task_ids.clone())
}

// Function to get the task ids
#[get("/task_ids")]
pub fn task_ids() -> Json<Vec<String>> {
    let task_ids = TASK_IDS.lock().unwrap();

    Json(task_ids.clone())
}

#[get("/queue")]
pub fn queue() -> Json<Vec<Value>> {
    // Fetch all the items in the queue, in the database in table db_queue
    let queue_items = get_all_from_db_queue().unwrap();

    // Create a vector to hold all the tasks as JSON objects
    let mut json_objects = Vec::new();

    // Iterate over the tasks
    for queue_item in queue_items {
        let id = queue_item.0.clone();
        let input_path = queue_item.1.clone();
        let output_path = queue_item.2.clone();
        let encoder = queue_item.3.clone();
        let preset = queue_item.4.clone();
        let vmaf_target = queue_item.5.clone();
        let vmaf_threads = queue_item.6.clone();

        // Serialize the values into a JSON object
        let json_object = json!({
            "id": id,
            "input_path": input_path,
            "output_path": output_path,
            "encoder": encoder,
            "preset": preset,
            "vmaf_target": vmaf_target,
            "vmaf_threads": vmaf_threads
        });

        // Add the JSON object to the vector
        json_objects.push(json_object);
    }

    Json(json_objects)
}

#[post("/add_to_queue", data = "<queue_input>")]
pub fn add_to_queue(queue_input: Json<QueueInput>) -> &'static str {
    // Add the task to the database
    let _task = add_to_db_queue(
        &queue_input.input_path,
        &queue_input.output_path,
        &queue_input.encoder,
        &queue_input.preset,
        &queue_input.vmaf_target,
        &queue_input.vmaf_threads,
    );

    "success"
}

#[post("/remove_from_queue", data = "<id>")]
pub fn remove_from_queue(id: Json<Value>) -> &'static str {
    // Remove the task from the database
    let _task = remove_from_db_queue(
        // Extract the id from the JSON
        id["id"].to_string().replace("\"", ""),
    );

    "success"
}
