build:    
    # Extract binary into hex for programmer
    cargo objcopy  -- -O ihex app.hex