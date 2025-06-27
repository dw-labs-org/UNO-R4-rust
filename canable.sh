set -e
sudo slcand -o -c -s8 "/dev/ttyACM$1" can0
sudo ip link set can0 up
sudo ip link set can0  txqueuelen 1000

# cansend can0 999#DEADBEEF   # Send a frame to 0x999 with payload 0xdeadbeef
# candump can0                # Show all traffic received by can0
# canbusload can0 500000      # Calculate bus loading percentage on can0 
# cansniffer can0             # Display top-style view of can traffic
# cangen can0 -D 11223344DEADBEEF -L 8    # Generate fixed-data CAN messages