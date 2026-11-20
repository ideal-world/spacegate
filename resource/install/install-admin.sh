BIN_PATH=/usr/local/bin
GATEWAY_CONFIG_DIR=/etc/spacegate/gateway/
# Rust 管理服务已迁至相邻仓库；保留旧安装入口的二进制安装兼容。
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ADMIN_ROOT=${SPACEGATE_ADMIN_ROOT:-"$SCRIPT_DIR/../../../spacegate-admin"}
cargo build --manifest-path "$ADMIN_ROOT/Cargo.toml" --target-dir "$ADMIN_ROOT/target" --bin spacegate-admin-server --release || exit $?
sudo cp "$ADMIN_ROOT/target/release/spacegate-admin-server" "$BIN_PATH"
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
