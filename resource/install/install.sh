CONFIG_PATH=/etc/spacegate/config.json
PLUGIN_PATH=/lib/spacegate/plugins
BIN_PATH=/usr/local/bin
cargo build --bin spacegate --release --features dylib
sudo cp target/release/spacegate $BIN_PATH

# Create config file
sudo mkdir /etc/spacegate

if [ -f $CONFIG_PATH ]; then
    echo "Config file already exists"
else
    sudo cp resource/install/default-config/config.json /etc/spacegate/config.json
fi

if [ -f $PLUGIN_PATH ]; then
    echo "Plugin dir already exists"
else
    # Create plugin directory
    sudo mkdir $PLUGIN_PATH
fi

if [ -f /etc/systemd/system/spacegate.service ]; then
    echo "Systemd unit file already exists"
else
    # Create systemd service
    sudo cp resource/install/spacegate.service /etc/systemd/system/spacegate.service
fi

# Enable and start service
sudo systemctl enable spacegate
sudo systemctl start spacegate
