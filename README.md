# Dragit
Experimental application for intuitive file sharing between devices.
In current state it uses Bluetooth OBEX protocol for file transfer throught the D-Bus BlueZ interface, so the app should work on most of the Linux devices.

## How to build and run?
1) Pair your devices (for instance laptop and a smartphone) using bluetooth. Make sure they are connected.
2) Compile and run program.
```
cargo run
```
3) Drag some file and drop it to the program window.
4) On the target device accept the incoming transfer.
5) Voila, you have transfered a file!


## TODOs:
- determine the position/orientation of the target device in relation to the sender - left/right, top/bottom
- use GTK tools to capture drop event on the edge of the screen
- add support for OS that don't use Bluez

