<p align="center"><img width="200" src="https://raw.githubusercontent.com/sireliah/dragit/master/static/logo_icon_t.svg" alt="Dragit - intuitive file sharing application"></p>

<h1 align="center">Dragit</h1>
<p align="center">Application for intuitive file sharing between devices.</p>

<p align="center">
<a href="https://github.com/sireliah/dragit/actions"><img src="https://github.com/sireliah/dragit/workflows/Build%20and%20Test/badge.svg" alt="Build Status"></a>
<a href="https://deps.rs/repo/github/sireliah/dragit"><img src="https://deps.rs/repo/github/sireliah/dragit/status.svg" alt="Dependency Status"></a>
<a href="https://github.com/sireliah/dragit/blob/master/LICENSE"><img src="https://img.shields.io/github/license/sireliah/dragit" alt="License"></a>
</p>

Dragit helps you share files between computers in the same network.

- useful when you want to send file from one computer to another
- requires no configuration
- single purpose - does only one thing and nothing more
- works on Linux and Windows machines

The application uses mDNS for automatic device discovery with help of `libp2p` library. The GUI is implemented in `gtk-rs`.

**Important note**: This is software in development phase and you should use it at your own risk.

## Contents
- [Preview](#preview)
- [How to install](#how-to-install)
    - [Using Flatpak](#using-flatpak)
    - [Download recent release](#download-recent-release)
- [How to use](#how-to-use)
- [Development](#development)
    - [How to build on Linux](#how-to-build-on-linux)
    - [How to build on Windows](#how-to-build-on-windows)
        - [Windows requirements](#windows-requirements)
    - [Performance](#performance)
    - [TODO](#todos)
    - [Done](#done)

## Preview
<img src="https://raw.githubusercontent.com/sireliah/dragit/master/static/dragit_screen.png" alt="Dragit screenshot" width=500px>

## How to install
### Using Flatpak
If you don't know how to use Flatpak yet, please follow [the setup guide](https://www.flatpak.org/setup/).

Then install Dragit as follows:
```bash
flatpak install com.sireliah.Dragit
```

### Download recent release
Alternatively you can download the latest [release](https://github.com/sireliah/dragit/releases/) for your OS and unpack it. Currently you can use `dragit` on 64-bit Linux and Windows (Please check [Windows requirements](#windows-requirements) for details).

## How to use
1. Start the application on two machines:

For Linux with Flatpak: run Dragit from installed applications.

For Linux executable:

```
./dragit
```

For Windows:
```
dragit.exe
```

You can run two `dragit` instances on the same machine for testing. No problem with that!

2. Wait for the two `dragit` instances to discover each other. You should see new drop zone area with IP address of the host.
3. Drag a file and drop it on the drop zone.
4. In the other window you will be asked whether you would like to accept the file. Probably you'd like to answer "Yes".
5. File will be transfered and saved in the `Downloads directory` (which is customizable).
6. Done!

![demo](./static/dragit.gif)

## Development
### How to build on Linux
```
cargo run
```

### How to build on Windows
`Dragit` works best on `x86_64-pc-windows-msvc` target. Detailed build instruction will be added in the future.

#### Windows requirements
It might happen that you don't have the `vcruntime140_1.dll` installed in your system and the application won't start. You can fix that by installing the [VC++ 2019 runtime dll](https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads). 

In the future releases this library will be installed automatically.

### Performance
Please build in the release mode to get the best performance (roughly 16-20x faster).

```
$ cargo build --release

$ ./target/release/dragit
```

### TODOs
#### Features
- find out how to use text drag&drop API on Windows with Gtk 
- show username in the device list
- have list of trusted devices
- add files queue
- re-enable the Bluetooth

#### Maintenance
- TCP retransmissions - what is wrong?
- fix the outbound memory issue (consumes too much memory on file reading)
- add Windows CI/CD
- add timeout on the Accept/Deny event
- inject_dial_upgrade_error - but why not inbound?
- adjust network timeouts

### Done
- show easy to understand instruction on startup
- show details about the host
- choose directory
- show version in the title bar
- crashes when no network interface is available
- implement error events
- TransferCommand::Accept should specify which file should be accepted
- add sender side progress bar
- fix the inbound memory issue
- add logging
- add test for the outbound/inbound
