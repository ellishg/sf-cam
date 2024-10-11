use anyhow::Result;
use embedded_svc::{http, io::Write};
use esp_idf_hal::{delay::FreeRtos, gpio::PinDriver};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop, hal::peripherals::Peripherals, http::server::EspHttpServer,
    log::EspLogger, nvs::EspDefaultNvsPartition, wifi,
};
use log::info;
use sf_cam::espcam::Camera;

#[derive(Debug)]
#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_password: &'static str,
}

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = wifi::BlockingWifi::wrap(
        wifi::EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;
    wifi.set_configuration(&wifi::Configuration::Client(
        wifi::ClientConfiguration::default(),
    ))?;
    wifi.start()?;

    let ap_infos = wifi.scan()?;
    info!("SSIDs {:?}", ap_infos);

    let ours = ap_infos.into_iter().find(|a| a.ssid == CONFIG.wifi_ssid);
    info!("Found {:?}", ours);

    let channel = if let Some(ours) = ours {
        info!(
            "Found configured access point {} on channel {}",
            CONFIG.wifi_ssid, ours.channel
        );
        Some(ours.channel)
    } else {
        info!(
            "Configured access point {} not found during scanning, will go with unknown channel",
            CONFIG.wifi_ssid
        );
        None
    };

    wifi.set_configuration(&wifi::Configuration::Client(wifi::ClientConfiguration {
        ssid: CONFIG.wifi_ssid.try_into().unwrap(),
        password: CONFIG.wifi_password.try_into().unwrap(),
        channel,
        // auth_method: AuthMethod::WPA2Personal,
        auth_method: wifi::AuthMethod::WPA,
        ..Default::default()
    }))?;

    info!("Connecting wifi...");

    wifi.connect()?;

    info!("Waiting for DHCP lease...");

    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    let mut server = EspHttpServer::new(&esp_idf_svc::http::server::Configuration::default())?;

    let mut camera_flash = PinDriver::output(peripherals.pins.gpio4)?;

    for _ in 0..2 {
        camera_flash.set_high()?;
        // we are sleeping here to make sure the watchdog isn't triggered
        FreeRtos::delay_ms(100);

        camera_flash.set_low()?;
        FreeRtos::delay_ms(500);
    }
    info!("start");

    let camera = Camera::new(
        peripherals.pins.gpio32, // pwdn
        peripherals.pins.gpio0,  // xclk
        peripherals.pins.gpio5,  // d0
        peripherals.pins.gpio18, // d1
        peripherals.pins.gpio19, // d2
        peripherals.pins.gpio21, // d3
        peripherals.pins.gpio36, // d4
        peripherals.pins.gpio39, // d5
        peripherals.pins.gpio34, // d6
        peripherals.pins.gpio35, // d7
        peripherals.pins.gpio25, // vsync
        peripherals.pins.gpio23, // href
        peripherals.pins.gpio22, // pclk
        peripherals.pins.gpio26, // sda
        peripherals.pins.gpio27, // scl
        // esp_idf_sys::camera::pixformat_t_PIXFORMAT_RGB565,
        // esp_idf_sys::camera::pixformat_t_PIXFORMAT_GRAYSCALE,
        esp_idf_sys::camera::pixformat_t_PIXFORMAT_JPEG,
        // esp_idf_sys::camera::framesize_t_FRAMESIZE_UXGA,
        esp_idf_sys::camera::framesize_t_FRAMESIZE_SVGA,
    )?;
    info!("initialized");

    server.fn_handler::<anyhow::Error, _>("/camera.jpg", http::Method::Get, move |request| {
        let framebuffer = camera.get_framebuffer();

        if let Some(framebuffer) = framebuffer {
            let data = framebuffer.data();

            let headers = [
                ("Content-Type", "image/jpeg"),
                ("Content-Length", &data.len().to_string()),
            ];
            let mut response = request.into_response(200, Some("OK"), &headers).unwrap();
            response.write_all(data)?;
        } else {
            let mut response = request.into_ok_response()?;
            response.write_all("no framebuffer".as_bytes())?;
        }

        Ok(())
    })?;

    server.fn_handler::<anyhow::Error, _>("/", http::Method::Get, |request| {
        info!("GET request");
        let mut response = request.into_ok_response()?;
        response.write_all("ok".as_bytes())?;
        Ok(())
    })?;

    // Keep wifi and the server running beyond when main() returns (forever)
    // Do not call this if you ever want to stop or access them later.
    // Otherwise you can either add an infinite loop so the main task
    // never returns, or you can move them to another thread.
    // https://doc.rust-lang.org/stable/core/mem/fn.forget.html
    core::mem::forget(wifi);
    core::mem::forget(server);

    Ok(())
}
