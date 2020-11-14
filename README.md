[![Actions Status](https://github.com/sireliah/dragit/workflows/Build%20and%20Test/badge.svg)](https://github.com/sireliah/dragit/actions)
# Dragit
Experimental application for intuitive file sharing between devices.

The network part is using `libp2p` and custom protocol for file transfer.
The frontend is ran on `gtk-rs`.

## How to build and run?
1) Start the application in two terminals (can be on separate machines in the same network)
```
$ cargo run
```
2) Wait for the two applications discover each other
3) Drag a file and drop it to the drop zone in one of the windows
4) File should be saved to your Downloads folder!

## Performance
Please build in the release mode to get the best performance (roughly 16-20x faster).

```
$ cargo build --release

$ ./target/release/dragit
```

## Bluetooth support
App can use Bluetooth OBEX protocol for file transfer through the D-Bus BlueZ interface, which should work on most of the Linux devices. (currently disabled)

## TODOs:
- crashes when no network interface is available
- add timeout on the Accept/Deny event
- implement error events
- fix the outbound memory issue (consumes too much memory on file reading)
- show details about the host
- add files queue
- inject_dial_upgrade_error - but why not inbound?

- adjust network timeouts
- re-enable the Bluetooth

## Done
- TransferCommand::Accept should specify which file should be accepted
- add sender side progress bar
- fix the inbound memory issue
- add logging
- add test for the outbound/inbound


## Windows
- vcruntime140_1.dll VC++ 2019 runtime dll. https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads