# Template for rust on Arduino UNO R4
See the justfile for commands to build and flash. It should be possible to use the arduino-cli to flash the hex file, although I have not tried.

Using the renesas bootloader requires pulling the boot pin to ground before reset, then flashing, then release boot pin and reset.

See [this post](https://domwil.co.uk/posts/uno-r4-rust/) for more details
