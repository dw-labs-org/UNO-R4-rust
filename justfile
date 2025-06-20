build:    
    rm app.hex || true
    # Extract binary into hex for programmer
    cargo objcopy  -- -O ihex app.hex

flash: build
    sudo -E env "PATH=$PATH" rfp-cli -device ra -t e2l -if swd -p app.hex -run

flash_usb: build
    rfp-cli -device ra -port /dev/ttyACM0 -p app.hex

flash_bl:
    rfp-cli -device ra -port /dev/ttyACM0 -p dfu_minima.hex