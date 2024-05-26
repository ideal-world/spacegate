BIN_PATH=/usr/local/bin
GATEWAY_CONFIG_DIR=/etc/spacegate/gateway/
cargo build --bin spacegate-admin-server --release
sudo cp target/release/spacegate-admin-server $BIN_PATH
if [ -d $GATEWAY_CONFIG_DIR ]; then
    echo "$GATEWAY_CONFIG_DIR existed"
else
    sudo mkdir $GATEWAY_CONFIG_DIR
fi
sudo cp -r resource/install/default-config/gateway/spacegate-admin /etc/spacegate/gateway
if [ -f /etc/systemd/system/spacegate-admin.service ]; then
    echo "Systemd unit file already exists"
else
    # Create systemd service
    sudo cp resource/install/spacegate-admin.service /etc/systemd/system/spacegate-admin.service
fi
sudo systemctl enable spacegate-admin
sudo systemctl start spacegate-admin
echo install finished
