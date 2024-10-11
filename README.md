# sf-cam

```
cargo install espup # Maybe?
cargo install ldproxy
cargo install espflash
```

* https://github.com/Kezii/esp32cam_rs/tree/master
* https://github.com/esp-rs/esp-idf-template/tree/master
* https://lastminuteengineers.com/getting-started-with-esp32-cam/
* https://github.com/espressif/esp32-camera

### Wokwi Simulation

#### VS Code Dev Containers and GitHub Codespaces

The Dev Container includes the Wokwi Vs Code installed, hence you can simulate your built projects doing the following:
1. Press `F1`
2. Run `Wokwi: Start Simulator`

> **Note**
>
>  We assume that the project is built in `debug` mode, if you want to simulate projects in release, please update the `elf` and  `firmware` proprieties in `wokwi.toml`.

For more information and details on how to use the Wokwi extension, see [Getting Started] and [Debugging your code] Chapter of the Wokwi documentation.

[Getting Started]: https://docs.wokwi.com/vscode/getting-started
[Debugging your code]: https://docs.wokwi.com/vscode/debugging
