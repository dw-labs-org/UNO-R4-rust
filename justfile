build:    
    rm app.hex || true
    # Extract binary into hex for programmer
    cargo objcopy  --release -- -O ihex app.hex

flash: build
    sudo -E env "PATH=$PATH" rfp-cli -device ra -t e2l -if swd -p app.hex -run

flash_usb: build
    rfp-cli -device ra -port /dev/ttyACM0 -p app.hex

flash_bl_usb:
    rfp-cli -device ra -port /dev/ttyACM0 -p dfu_minima.hex

flash_bl:
    sudo -E env "PATH=$PATH" rfp-cli -device ra -t e2l -if swd -p dfu_minima.hex 

show_asm:
    cargo asm  --bin uno-r4-rust __cortex_m_rt_main  --intel > app.asm

serial:
    sudo tio -b 115200 /dev/ttyUSB0 --input-mode line -et --map ICRNL,INLCRNL