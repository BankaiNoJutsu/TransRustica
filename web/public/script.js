window.onload = function() {
    fetchProgress();
    fetchProgressScan();
    fetchQueue();
    addToQueueTable();
    setInterval(fetchProgress, 500); // Fetch progress every half second
    setInterval(fetchProgressScan, 1000); // Fetch progress every second
    setInterval(fetchQueue, 1000); // Fetch queue every second
    setInterval(addToQueueTable, 1000); // Fetch queue every second
    setInterval(sendMessageToClient, 1000);
    //buildHtmlTable();
};

var wsUri = "ws://127.0.0.1:8000/echo?stream";
var log;

function init() {
    log = document.getElementById("log");
    message = document.getElementById("message");

    testWebSocket();
}

function testWebSocket() {
    websocket = new WebSocket(wsUri);
    websocket.onopen = onOpen;
    websocket.onclose = onClose;
    websocket.onerror = onError;
}

function onOpen(evt) {
    writeLog("CONNECTED");
}

function onClose(evt) {
    writeLog("Websocket DISCONNECTED");
}

function onError(evt) {
    writeLog('<span style="color: red;">ERROR:</span> ' + evt.data);
}

function sendMessage(message) {
    writeLog(message);
    websocket.send(message);
}

function writeLog(message) {
    var pre = document.createElement("p");
    pre.innerHTML = message;
    log.prepend(pre);
    //console.log(message);
}

window.addEventListener("load", init, false);

// TODO
// add type, CRF, file/total, VMAF, % of file, speed, codec information
// put it on a single line
// show "Calculating VMAF target" if values are not being updated

let taskIds = [];

function fetchTaskIds() {
    fetch('/task_ids')
        .then(response => response.json())
        .then(data => {
            taskIds = data;
        })
        .catch(error => console.error('Error fetching task IDs:', error));
}

function sendMessageToClient() {
    fetch('/progress')
    .then(response => response.json())
    .then(data => {
        sendMessage(JSON.stringify(data));
    })
}

function fetchProgress() {
    fetch('/progress')
        .then(response => response.json())
        .then(data => {
            // Check if the table already exists
            let taskTable = document.getElementById(`task-table`);
            if (!taskTable) {
                taskTable = document.createElement('table');
                taskTable.id = `task-table`;
                taskTable.innerHTML = `
                    <thead>
                        <tr>
                            <th>Task ID</th>
                            <th>FPS</th>
                            <th>Frame</th>
                            <th>Frames</th>
                            <th>Percentage</th>
                            <th>Size</th>
                            <th>Current File Count</th>
                            <th>Total Files</th>
                            <th>Current File Name</th>
                        </tr>
                    </thead>
                    <tbody></tbody>
                `;
                document.body.appendChild(taskTable);
            }
            
            // Loop through all task IDs and create a new row for each one
            for (id in data) {
                if (data.id) {
                    // Check if the row already exists
                    let taskRow = document.getElementById(`task-row-${data.id}`);
                    if (!taskRow) {
                        taskRow = document.createElement('tr');
                        taskRow.id = `task-row-${data.id}`;
                        taskRow.innerHTML = `
                            <td><span id="id-${data.id}">${data.id}</span></td>
                            <td><span id="fps-${data.id}">${data.fps}</span></td>
                            <td><span id="frame-${data.id}">${data.frame}</span></td>
                            <td><span id="frames-${data.id}">${data.frames}</span></td>
                            <td><span id="percentage-${data.id}">${parseFloat(data.percentage).toFixed(2)}%</span></td>
                            <td><span id="size-${data.id}">${parseFloat(data.size).toFixed(2)} MB</span></td>
                            <td><span id="current_file_count-${data.id}">${data.current_file_count}</span></td>
                            <td><span id="total_files-${data.id}">${data.total_files}</span></td>
                            <td><span id="current_file_name-${data.id}">${data.current_file_name}</span></td>
                        `;
                        taskTable.querySelector('tbody').appendChild(taskRow);
                    } else {
                        // Update the row values
                        document.getElementById(`id-${data.id}`).textContent = data.id;
                        document.getElementById(`fps-${data.id}`).textContent = data.fps;
                        document.getElementById(`frame-${data.id}`).textContent = data.frame;
                        document.getElementById(`frames-${data.id}`).textContent = data.frames;
                        document.getElementById(`percentage-${data.id}`).textContent = parseFloat(data.percentage).toFixed(2) + '%';
                        document.getElementById(`size-${data.id}`).textContent = parseFloat(data.size).toFixed(2) + ' MB';
                        document.getElementById(`current_file_count-${data.id}`).textContent = data.current_file_count;
                        document.getElementById(`total_files-${data.id}`).textContent = data.total_files;
                        document.getElementById(`current_file_name-${data.id}`).textContent = data.current_file_name;
                    }
                }
            }
        })
        .catch(error => console.error('Error fetching progress:', error));
}

function fetchProgressScan() {
    fetch('/progress_scan')
        .then(response => response.json())
        .then(data => {
            document.getElementById('scan_progress').textContent = data.total;
        })
        .catch(error => console.error('Error fetching progress:', error));
}

let jobs = [];

function startTranscodingWithInput() {
    const input_path = document.getElementById('input_path').value;
    const output_path = document.getElementById('output_path').value;
    const encoder = document.getElementById('encoder').value;
    const preset = document.getElementById('preset').value;
    const vmaf_target = document.getElementById('vmaf-target').value;
    const vmaf_threads = document.getElementById('vmaf-threads').value;

    let job = {
        input_path: input_path,
        output_path: output_path,
        encoder: encoder,
        preset: preset,
        vmaf_target: vmaf_target,
        vmaf_threads: vmaf_threads
    };

    fetch('/start_transcode', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(job),
    })
    .then(response => {
        if (response.ok) {
            console.log('Transcoding job started');
            console.log(response);
        } else {
            console.error('Failed to start transcoding job');
        }
    })
    .catch(error => {
        console.error('Error starting transcoding job:', error);
    });
}

function startScanWithInput() {
    const input_path = document.getElementById('input_path_scan').value;

    // Convert the input path to a base64 string
    var base64_encoded_input_path = window.btoa(input_path);

    fetch(`/scan/${base64_encoded_input_path}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            input_path: base64_encoded_input_path,
        })
    })
    .then(response => {
        if (response.ok) {
            console.log('Scan job started');
            console.log(response);
        } else {
            console.error('Failed to start scan job');
        }
    })
    .catch(error => {
        console.error('Error starting scan job:', error);
    })
}

// Function to build a HTML Table from the fetching data from /all endpoint, which returns a JSON object
function buildHtmlTable() {
    const table = document.createElement('table');
    const thead = document.createElement('thead');
    const tbody = document.createElement('tbody');
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    const td = document.createElement('td');

    table.appendChild(thead);
    table.appendChild(tbody);
    fetch('/all')
        .then(response => response.json())
        .then(data => {
            data.forEach(item => {
                const tr = document.createElement('tr');
                const td = document.createElement('td');
                const a = document.createElement('a');
                a.textContent = item.id;
                a.href = item.filepath;
                td.appendChild(a);
                tr.appendChild(td);
                tbody.appendChild(tr);
            });
        })
    document.body.appendChild(table);
}

// Create a function that adds the form input to a queue
function addToQueue() {
    const input_path = document.getElementById('input_path').value;
    const output_path = document.getElementById('output_path').value;
    const encoder = document.getElementById('encoder').value;
    const preset = document.getElementById('preset').value;
    const vmaf_target = document.getElementById('vmaf-target').value;
    const vmaf_threads = document.getElementById('vmaf-threads').value;

    // Post the form data to the /add_to_queue endpoint
    fetch('/add_to_queue', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            input_path: input_path,
            output_path: output_path,
            encoder: encoder,
            preset: preset,
            vmaf_target: vmaf_target,
            vmaf_threads: vmaf_threads
        }),
    })
    .then(response => {
        if (response.ok) {
            console.log('Job added to queue');
            console.log(response);
        } else {
            console.error('Failed to add job to queue');
        }
    })
    .catch(error => {
        console.error('Error adding job to queue:', error);
    })
}

function fetchQueue() {
    fetch('/queue')
        .then(response => response.json())
        .then(data => {
            //console.log(data);
        })
        .catch(error => console.error('Error fetching queue:', error));
}

// Function to build a table from the addToQueue functionfunction addToQueueTable() {
function addToQueueTable() {
    // Get the table, create it if it doesn't exist
    let table = document.getElementById('queueTable');
    if (!table) {
        table = document.createElement('table');
        table.id = 'queueTable'; // Set the id for the new table
        const thead = document.createElement('thead');
        const tbody = document.createElement('tbody');
        table.appendChild(thead);
        table.appendChild(tbody);
        document.body.appendChild(table);
    }

    const tbody = table.querySelector('tbody');

    fetch('/queue')
        .then(response => response.json())
        .then(data => {
            // Remove all existing rows
            while (tbody.firstChild) {
                tbody.removeChild(tbody.firstChild);
            }

            data.forEach(item => {
                const tr = document.createElement('tr');

                // Create a new td for each property and append it to the tr
                Object.keys(item).forEach(key => {
                    const td = document.createElement('td');
                    td.textContent = item[key];
                    tr.appendChild(td);
                });

                // Add a button to remove the item from the queue
                const td = document.createElement('td');
                const remove = document.createElement('button');
                remove.textContent = 'X';
                remove.style.backgroundColor = 'red'; // Make the button red
                remove.style.color = 'white'; // Make the text color white for better visibility

                // Add hover effect
                remove.onmouseover = function() {
                    this.style.backgroundColor = 'darkred'; // Change color on hover
                }
                remove.onmouseout = function() {
                    this.style.backgroundColor = 'red'; // Change color back when not hovering
                }
                remove.onclick = () => {
                    fetch('/remove_from_queue', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                        },
                        body: JSON.stringify({
                            id: item.id,
                        }),
                    })
                    .then(response => {
                        if (response.ok) {
                            console.log('Job removed from queue');
                            console.log(response);
                            tr.remove(); // Remove the row from the table
                        } else {
                            console.error('Failed to remove job from queue');
                        }
                    })
                    .catch(error => {
                        console.error('Error removing job from queue:', error);
                    })
                }

                // Add a button to start transcoding
                const start = document.createElement('button');
                start.textContent = 'Start';
                start.onclick = () => {
                    fetch('/start_transcode', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                        },
                        body: JSON.stringify({
                            input_path: item.input_path,
                            output_path: item.output_path,
                            encoder: item.encoder,
                            preset: item.preset,
                            vmaf_target: item.vmaf_target,
                            vmaf_threads: item.vmaf_threads
                        }),
                    })
                    .then(response => {
                        if (response.ok) {
                            console.log('Job started transcoding');
                            console.log(response);
                        } else {
                            console.error('Failed to start transcoding job');
                        }
                    })
                    .catch(error => {
                        console.error('Error starting transcoding job:', error);
                    })
                }
                td.appendChild(start);
                td.appendChild(remove);
                tr.appendChild(td);
                tbody.appendChild(tr);                
            });
        })
}