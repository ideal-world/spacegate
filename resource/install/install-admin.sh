RUN_ID=8982455335
ARTIFACTS_ID=1479473822
BIN_PATH=/usr/local/bin
cargo build --bin spacegate-admin-server --release 
sudo cp target/release/spacegate-admin-server $BIN_PATH

(curl https://github.com/ideal-world/spacegate-admin-fe/actions/runs/$RUN_ID/artifacts/$ARTIFACTS_ID) | funzip | tar -x -C /var/www/spacegate

