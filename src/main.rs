use anyhow::Result;
use esp_idf_hal::{delay::FreeRtos, gpio::PinDriver, peripherals::Peripherals};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop, http, io::Write, log::EspLogger, nvs::EspDefaultNvsPartition,
    wifi,
};
use log::info;
use sf_cam::esp_camera::Camera;

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

    let mut camera_flash = PinDriver::output(peripherals.pins.gpio4)?;

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

    server.fn_handler::<anyhow::Error, _>("/", http::Method::Get, |request| {
        info!("GET request /");
        let mut response = request.into_ok_response()?;
        response.write_all("ok".as_bytes())?;
        Ok(())
    })?;

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
        10_000_000,              // xclk_freq_hz
        esp_idf_sys::camera::pixformat_t_PIXFORMAT_JPEG,
        esp_idf_sys::camera::framesize_t_FRAMESIZE_UXGA, // 1600x1200
        12,                                              // jpeg_quality
        esp_idf_sys::camera::camera_fb_location_t_CAMERA_FB_IN_PSRAM,
        esp_idf_sys::camera::camera_grab_mode_t_CAMERA_GRAB_WHEN_EMPTY,
    )?;

    let mut buffers: Vec<Vec<_>> = vec![];
    for _ in 0..5 {
        FreeRtos::delay_ms(500);
        let buffer = camera.get_framebuffer().unwrap();
        buffers.push(buffer.data().to_vec());
    }

    for (i, buffer) in buffers.into_iter().enumerate() {
        let uri = format!("/camera_{}.jpg", i);
        server.fn_handler::<anyhow::Error, _>(uri.as_str(), http::Method::Get, move |request| {
            let headers = [
                ("Content-Type", "image/jpeg"),
                ("Content-Length", &buffer.len().to_string()),
            ];
            let mut response = request.into_response(200, Some("OK"), &headers).unwrap();
            response.write_all(buffer.as_slice())?;

            Ok(())
        })?;
    }

    server.fn_handler::<anyhow::Error, _>("/camera.jpg", http::Method::Get, move |request| {
        info!("GET request /camera.jpg");
        let framebuffer = camera.get_framebuffer();

        if let Some(framebuffer) = framebuffer {
            let data = framebuffer.data();
            let framebuffer_size = byte_unit::Byte::from_u64(data.len() as u64)
                .get_appropriate_unit(byte_unit::UnitType::Decimal);
            info!("Framebuffer size: {}", framebuffer_size);

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

    server.fn_handler::<anyhow::Error, _>("/camera.html", http::Method::Get, move |request| {
        let mut response = request.into_ok_response()?;
        response.write_all(
            "
        <img src=\"/camera_0.jpg\">
        <img src=\"/camera_1.jpg\">
        <img src=\"/camera_2.jpg\">
        <img src=\"/camera_3.jpg\">
        <img src=\"/camera_4.jpg\">
        "
            .as_bytes(),
        )?;
        Ok(())
    })?;

    // Flash to know that we have connected to wifi and the server is setup.
    camera_flash.set_high()?;
    FreeRtos::delay_ms(100);
    camera_flash.set_low()?;

    // Keep wifi and the server running beyond when main() returns (forever)
    // Do not call this if you ever want to stop or access them later.
    // Otherwise you can either add an infinite loop so the main task
    // never returns, or you can move them to another thread.
    // https://doc.rust-lang.org/stable/core/mem/fn.forget.html
    core::mem::forget(wifi);
    core::mem::forget(server);

    Ok(())
}
