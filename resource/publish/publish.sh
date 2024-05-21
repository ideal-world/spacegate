VERSION=$1
PACKAGE_NAME=$2
echo "Publishing version $VERSION of $PACKAGE_NAME";
cargo search "$PACKAGE_NAME" --registry crates-io | grep -q "$PACKAGE_NAME = \"$VERSION\""
if [ $? -eq 0 ]; then
    echo "Version $VERSION of $PACKAGE_NAME is already published."
else
    cargo package -p $PACKAGE_NAME 
    cargo publish -p $PACKAGE_NAME --registry crates-io
fi