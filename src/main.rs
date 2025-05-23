use anyhow::Result;
use esp_idf_hal::delay::Delay;
use esp_idf_hal::gpio;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::fs::fatfs::Fatfs;
use esp_idf_svc::hal::sd::{
    mmc::SdMmcHostConfiguration, mmc::SdMmcHostDriver, SdCardConfiguration, SdCardDriver,
};
use esp_idf_svc::io::vfs::MountedFatfs;
use esp_idf_svc::io::Write;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::{http, wifi};
use log::info;
use sf_cam::esp_camera::Camera;
use std::time::Duration;

#[derive(Debug)]
#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_password: &'static str,
    #[default("1h")]
    timelapse_period: &'static str,
    #[default(6)]
    picture_count: u32,
}

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    EspLogger::initialize_default();
    info!("Logging initialized");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let delay = Delay::new_default();

    // TODO: This pin conflicts with the sd card
    // https://dr-mntn.net/2021/02/using-the-sd-card-in-1-bit-mode-on-the-esp32-cam-from-ai-thinker
    // let mut camera_flash = gpio::PinDriver::output(pins.gpio4)?;
    // camera_flash.set_low()?;

    let sd_card_driver = SdCardDriver::new_mmc(
        SdMmcHostDriver::new_slot1_4bits(
            peripherals.sdmmc1,
            pins.gpio15, // cmd
            pins.gpio14, // clk
            pins.gpio2,  // d0
            pins.gpio4,  // d1
            pins.gpio12, // d2
            pins.gpio13, // d3
            None::<gpio::AnyIOPin>,
            None::<gpio::AnyIOPin>,
            &SdMmcHostConfiguration::new(),
        )?,
        // TODO: We can also use Data width bit 1 to avoid using gpio 4 connect to the flash LED, but it might be slower.
        // SdMmcHostDriver::new_slot1_1bit(
        //     peripherals.sdmmc1,
        //     pins.gpio15,
        //     pins.gpio14,
        //     pins.gpio2,
        //     None::<gpio::AnyIOPin>,
        //     None::<gpio::AnyIOPin>,
        //     &SdMmcHostConfiguration::new(),
        // )?,
        &SdCardConfiguration::new(),
    )?;

    // Apparently filenames must have the 8.3 name format.
    // https://en.wikipedia.org/wiki/8.3_filename
    let fatfs = MountedFatfs::mount(Fatfs::new_sdcard(0, sd_card_driver)?, "/SDCARD", 4)?;
    info!("SD card mounted");

    let mut wifi = wifi::BlockingWifi::wrap(
        wifi::EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;
    wifi.set_configuration(&wifi::Configuration::Client(
        wifi::ClientConfiguration::default(),
    ))?;
    wifi.start()?;

    let wifi_scan = wifi.scan()?;
    let wifi_ap = wifi_scan
        .into_iter()
        .find(|ap| ap.ssid == CONFIG.wifi_ssid)
        .expect("Unable to find SSID");

    wifi.set_configuration(&wifi::Configuration::Client(wifi::ClientConfiguration {
        ssid: CONFIG.wifi_ssid.try_into().unwrap(),
        password: CONFIG.wifi_password.try_into().unwrap(),
        channel: Some(wifi_ap.channel),
        auth_method: wifi_ap
            .auth_method
            .unwrap_or(wifi::AuthMethod::WPA2Personal),
        ..Default::default()
    }))?;

    wifi.connect()?;

    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    let mut server = http::server::EspHttpServer::new(&http::server::Configuration::default())?;

    server.fn_handler::<anyhow::Error, _>("/", http::Method::Get, move |request| {
        info!("GET request /");
        if std::fs::exists("/SDCARD/LATEST")? {
            let data = std::fs::read("/SDCARD/LATEST")?;
            let headers = [
                ("Content-Type", "image/jpeg"),
                ("Content-Length", &data.len().to_string()),
            ];
            let mut response = request.into_response(200, Some("OK"), &headers)?;
            response.write_all(data.as_slice())?;
        } else {
            let mut response = request.into_ok_response()?;
            response.write_all("No image yet.".as_bytes())?;
        }
        Ok(())
    })?;

    let camera = Camera::new(
        pins.gpio32, // pwdn
        pins.gpio0,  // xclk
        pins.gpio5,  // d0
        pins.gpio18, // d1
        pins.gpio19, // d2
        pins.gpio21, // d3
        pins.gpio36, // d4
        pins.gpio39, // d5
        pins.gpio34, // d6
        pins.gpio35, // d7
        pins.gpio25, // vsync
        pins.gpio23, // href
        pins.gpio22, // pclk
        pins.gpio26, // sda
        pins.gpio27, // scl
        10_000_000,  // xclk_freq_hz
        esp_idf_sys::camera::pixformat_t_PIXFORMAT_JPEG,
        esp_idf_sys::camera::framesize_t_FRAMESIZE_UXGA, // 1600x1200
        12,                                              // jpeg_quality
        esp_idf_sys::camera::camera_fb_location_t_CAMERA_FB_IN_PSRAM,
        esp_idf_sys::camera::camera_grab_mode_t_CAMERA_GRAB_WHEN_EMPTY,
    )?;

    // Flash to know that we have connected to wifi and the server is setup.
    // camera_flash.set_high()?;
    // delay.delay_ms(100);
    // camera_flash.set_low()?;

    let timelapse_period = CONFIG
        .timelapse_period
        .parse::<humantime::Duration>()?
        .into();
    generate_timelapse(camera, timelapse_period, CONFIG.picture_count, delay)?;

    core::mem::forget(wifi);
    core::mem::forget(server);
    core::mem::forget(fatfs);

    Ok(())
}

fn generate_timelapse(
    camera: Camera,
    timelapse_period: Duration,
    picture_count: u32,
    delay: Delay,
) -> Result<()> {
    assert!(picture_count > 0);

    let delay_time = timelapse_period / picture_count;
    for i in 0..picture_count - 1 {
        capture_picture(i, &camera)?;
        delay.delay_ms(delay_time.as_millis().try_into()?);
    }
    // We don't want a delay after the last picture.
    capture_picture(picture_count, &camera)?;

    // TODO: We need a lightweight way of generating a timelapse mp4.
    Ok(())
}

fn capture_picture(i: u32, camera: &Camera) -> Result<()> {
    let framebuffer = camera.get_framebuffer().unwrap();
    std::fs::write(format!("/SDCARD/P{}", i), framebuffer.data())?;
    std::fs::write("/SDCARD/LATEST", framebuffer.data())?;
    Ok(())
}
