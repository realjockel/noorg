#!/bin/bash

VERSION=$(grep version Cargo.toml | head -n 1 | cut -d '"' -f 2)
RELEASE_DIR="target/release"

check_permissions() {
    if [ "$EUID" -ne 0 ]; then
        echo "Please run as root or use sudo"
        exit 1
    fi
}

install_unix() {
    check_permissions
    echo "Building for Unix-like system..."
    cargo build --release
    
    # Create application directories
    mkdir -p /usr/local/bin
    mkdir -p /usr/local/share/noorg/{bin,resources}
    
    # Copy binaries
    echo "Installing noorg binaries..."
    cp "$RELEASE_DIR/note_tray" /usr/local/share/noorg/bin/
    cp "$RELEASE_DIR/note_cli" /usr/local/share/noorg/bin/
    cp "$RELEASE_DIR/note_settings" /usr/local/share/noorg/bin/
    
    # Set permissions
    chmod +x /usr/local/share/noorg/bin/*
    
    # Create command entry point with CLI support
    echo "Creating noorg command..."
    cat > /tmp/noorg << 'EOF'
#!/bin/bash
cd /usr/local/share/noorg

if [ "$1" = "note_cli" ]; then
    shift  # Remove 'note_cli' from the arguments
    exec bin/note_cli "$@"
elif [ "$1" = "settings" ]; then
    exec bin/note_settings "$@"
else
    exec bin/note_tray "$@"
fi
EOF
    
    mv /tmp/noorg /usr/local/bin/noorg
    chmod +x /usr/local/bin/noorg
}

install_windows() {
    echo "Building for Windows..."
    cargo build --release
    
    # Create application directories
    mkdir -p "C:/Program Files/noorg/bin"
    mkdir -p "C:/Program Files/noorg/resources"
    
    # Copy binaries
    echo "Installing noorg binaries..."
    cp "$RELEASE_DIR/note_tray.exe" "C:/Program Files/noorg/bin/"
    cp "$RELEASE_DIR/note_cli.exe" "C:/Program Files/noorg/bin/"
    cp "$RELEASE_DIR/note_settings.exe" "C:/Program Files/noorg/bin/"
    
    # Add to PATH
    setx PATH "%PATH%;C:\Program Files\noorg\bin"
}

uninstall() {
    check_permissions
    case "$(uname -s)" in
        Darwin*|Linux*)
            echo "Uninstalling noorg..."
            rm -f /usr/local/bin/noorg
            rm -rf /usr/local/share/noorg
            ;;
        MINGW*|MSYS*|CYGWIN*)
            echo "Uninstalling noorg..."
            rm -rf "C:/Program Files/noorg"
            ;;
        *)
            echo "Unsupported platform"
            ;;
    esac
    echo "✅ noorg uninstalled successfully"
}

case "$1" in
    uninstall)
        uninstall
        ;;
    *)
        case "$(uname -s)" in
            Darwin*|Linux*)    install_unix ;;
            MINGW*|MSYS*|CYGWIN*)    install_windows ;;
            *)          echo "Unsupported platform" ;;
        esac
        ;;
esac