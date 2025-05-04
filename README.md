# sf-cam
[![CI](https://github.com/ellishg/sf-cam/actions/workflows/ci.yml/badge.svg)](https://github.com/ellishg/sf-cam/actions/workflows/ci.yml)

## Description
The goal of this project was to use the esp32-cam to capture a picture N times a day, generate a timelapse, and upload it to social media. My vision was that I would find a fantastic view of San Francisco and generate beautiful timelapses every day. The roadblock I ran into was I could not generate a video from a set of images without running out of memory.

## Dependencies
```
cargo install ldproxy
cargo install espflash
```

## Links
* https://github.com/jlocash/esp-camera-rs
* https://lastminuteengineers.com/getting-started-with-esp32-cam/
* https://github.com/espressif/esp32-camera
* SDMMC
  * https://github.com/espressif/esp-idf/tree/master/examples/storage/sd_card/sdmmc
  * https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/storage/sdmmc.html
  * https://crates.io/crates/embedded-sdmmc
  