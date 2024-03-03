# transrustica
transrustica

For file:
```
.\cli.exe -m default -e hevc_nvenc -v 97 --verbose -t 2 -o "." -i ".\onepiece_demo.mp4"
```
For folder (folder1 containing onepiece_demo.mp4):
```
.\cli.exe -m default -e hevc_nvenc -v 97 --verbose -t 2 -o "A:\temp\" --inputpath "A:\folder\folder1\"
```
```
Usage: cli.exe [OPTIONS] --inputpath <INPUTPATH>

Options:
  -i, --inputpath <INPUTPATH>
          input video path folder path (\\... or /... or C:\...)
  -o, --outputpath <OUTPUTPATH>
          output video path folder path (\\... or /... or C:\...) (default: .) [default: .]
  -v, --vmaf <VMAF>
          VMAF target value (default: 97) [default: 97]
  -e, --encoder <ENCODER>
          Encoder to use (default: libx265) (possible values: libx265, av1, libsvtav1, hevc_nvenc, hevc_qsv, av1_qsv) [default: libx265]
  -o, --output-folder <OUTPUT_FOLDER>
          [default: .]
      --verbose

  -m, --mode <MODE>
          Which mode to use for processing (default: default) (possible values: default, chunked) [default: default]
  -p, --vmaf-pool <VMAF_POOL>
          Which vmaf pool method to use (default: mean) (possible values: min, harmonic_mean, mean) [default: mean]
  -t, --vmaf-threads <VMAF_THREADS>
          [default: 2]
  -S, --vmaf-subsample <VMAF_SUBSAMPLE>
          Every n frame to subsample in the vmaf calculation (default: 1) [default: 1]
      --pix-fmt <PIX_FMT>
          Pixel format to use (default: yuv420p10le) [default: yuv420p10le]
      --max-crf <MAX_CRF>
          Max CRF value (default: 28) (possible values: 0-51) [default: 28]
      --sample-every <SAMPLE_EVERY>
          Sample every Nth minute (default: 3m) [default: 3m]
      --params-ab-av1 <PARAMS_AB_AV1>
          Params for ab-av1 (default: limit-sao,bframes=8,psy-rd=1,aq-mode=3) [default: x265-params=limit-sao,bframes=8,psy-rd=1,aq-mode=3]
      --params-x265 <PARAMS_X265>
          Params for x265 (default: -x265-params limit-sao:bframes=8:psy-rd=1:aq-mode=3) [default: "-x265-params limit-sao:bframes=8:psy-rd=1:aq-mode=3"]
      --preset-x265 <PRESET_X265>
          Preset for x265 (default: slow) (possible values: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow, placebo) [default: slow]
      --preset-av1 <PRESET_AV1>
          Preset for av1 (default: 8) [default: 8]
      --preset-hevc-nvenc <PRESET_HEVC_NVENC>
          Preset for hevc_nvenc (default: p7) [default: p7]
      --params-hevc-nvenc <PARAMS_HEVC_NVENC>
          Params for hevc_nvenc (default: -rc-lookahead 100 -b_ref_mode each -tune hq) [default: "-rc-lookahead 100 -b_ref_mode each -tune hq"]
      --preset-hevc-qsv <PRESET_HEVC_QSV>
          Preset for hevc_qsv (default: veryslow) [default: veryslow]
      --preset-av1-qsv <PRESET_AV1_QSV>
          Preset for av1_qsv (default: veryslow) [default: 1]
      --params-hevc-qsv <PARAMS_HEVC_QSV>
          Params for hevc_qsv (default: -init_hw_device qsv=intel,child_device=0 -b_strategy 1 -look_ahead 1 -async_depth 100) [default: "-init_hw_device qsv=intel,child_device=0 -b_strategy 1 -look_ahead 1 -async_depth 100"]
      --params-av1-qsv <PARAMS_AV1_QSV>
          Params for av1_qsv (default: -init_hw_device qsv=intel,child_device=0 -g 256 -b_strategy 1 -look_ahead 1 -async_depth 100) [default: "-init_hw_device qsv=intel,child_device=0 -b_strategy 1 -look_ahead 1 -async_depth 100"]
      --preset-libsvtav1 <PRESET_LIBSVTAV1>
          Preset for libsvtav1 (default: 5) (possible values: -2 - 13) [default: 5]
      --params-libsvtav1 <PARAMS_LIBSVTAV1>
          Params for libsvtav1 (default: ) [default: ]
      --preset-libaom-av1 <PRESET_LIBAOM_AV1>
          Preset for libaom-av1 (default: 4) [default: 4]
      --params-libaom-av1 <PARAMS_LIBAOM_AV1>
          Params for libaom-av1 (default: ) [default: ]
  -s, --scene-split-min <SCENE_SPLIT_MIN>
          Scene split minimum seconds (default: 2) [default: 2]
  -d, --task-id <TASK_ID>
          Task ID (default: "") [default: ]
  -h, --help
          Print help
  -V, --version
          Print version
```
