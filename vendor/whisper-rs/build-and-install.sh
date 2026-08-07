#!/bin/bash
set -e

# Build and install whisper-rs to HANDSFREEVC_DEV_HOME
# Similar layout to whisper.cpp installation

if [ -z "$HANDSFREEVC_DEV_HOME" ]; then
    echo "Error: HANDSFREEVC_DEV_HOME environment variable is not set"
    exit 1
fi

INSTALL_ROOT="${HANDSFREEVC_DEV_HOME}/whisper-rs"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Building whisper-rs for multiple platforms..."
echo "    Repository: $REPO_ROOT"
echo "    Install to: $INSTALL_ROOT"

# Create directory structure
mkdir -p "${INSTALL_ROOT}/include"
mkdir -p "${INSTALL_ROOT}/src/whisper_state"
mkdir -p "${INSTALL_ROOT}/sys/src"
mkdir -p "${INSTALL_ROOT}/linux-x86_64"
mkdir -p "${INSTALL_ROOT}/android/arm64-v8a"
mkdir -p "${INSTALL_ROOT}/android/armeabi-v7a"
mkdir -p "${INSTALL_ROOT}/android/x86_64"
mkdir -p "${INSTALL_ROOT}/android/x86"

cd "$REPO_ROOT"

# Copy Cargo package files
echo ""
echo "==> Copying Cargo package files..."
cp -v Cargo.toml "${INSTALL_ROOT}/"
cp -v build.rs "${INSTALL_ROOT}/"
cp -v LICENSE "${INSTALL_ROOT}/" 2>/dev/null || true

# Copy source files
echo "    Copying src/ directory..."
cp -v src/*.rs "${INSTALL_ROOT}/src/"
cp -v src/whisper_state/*.rs "${INSTALL_ROOT}/src/whisper_state/"

# Copy sys package files
echo "    Copying sys/ package..."
cp -v sys/Cargo.toml "${INSTALL_ROOT}/sys/"
cp -v sys/build.rs "${INSTALL_ROOT}/sys/"
cp -v sys/wrapper.h "${INSTALL_ROOT}/sys/"
cp -v sys/src/*.rs "${INSTALL_ROOT}/sys/src/"

# Build for Linux x86_64 (native)
echo ""
echo "==> Building for Linux x86_64..."
cargo build --release
echo "    Installing to ${INSTALL_ROOT}/linux-x86_64/"
cp -v target/release/libwhisper_rs.rlib "${INSTALL_ROOT}/linux-x86_64/"

# Build for Android targets if requested
if [ "$1" == "--with-android" ] || [ "$1" == "--all" ]; then
    # Check if Android targets are installed
    ANDROID_TARGETS=(
        "aarch64-linux-android:arm64-v8a"
        "armv7-linux-androideabi:armeabi-v7a"
        "x86_64-linux-android:x86_64"
        "i686-linux-android:x86"
    )

    for target_pair in "${ANDROID_TARGETS[@]}"; do
        IFS=':' read -r rust_target android_abi <<< "$target_pair"

        # Check if target is installed
        if rustup target list | grep -q "^${rust_target} (installed)"; then
            echo ""
            echo "==> Building for Android ${android_abi} (${rust_target})..."
            cargo build --release --target "${rust_target}"
            echo "    Installing to ${INSTALL_ROOT}/android/${android_abi}/"
            cp -v "target/${rust_target}/release/libwhisper_rs.rlib" "${INSTALL_ROOT}/android/${android_abi}/"
        else
            echo ""
            echo "==> Skipping ${rust_target} (not installed)"
            echo "    To install: rustup target add ${rust_target}"
        fi
    done
fi

# Copy header files (public API from src/lib.rs - for FFI consumers)
echo ""
echo "==> Copying header files..."
# Note: whisper-rs is a Rust library, so there's no traditional C header
# But we'll document the public API
cat > "${INSTALL_ROOT}/include/README.md" << 'EOF'
# whisper-rs Headers

whisper-rs is a Rust library that provides safe bindings to whisper.cpp.

## Usage in Rust Projects

Add to your `Cargo.toml`:

```toml
[dependencies]
whisper-rs = { path = "${HANDSFREEVC_DEV_HOME}/whisper-rs" }
```

Or reference the installed artifacts directly in your build system.

## Public API

The main types and functions are exported from `whisper_rs` crate:

- `WhisperContext` - Main context for loading models
- `WhisperState` - State for running inference
- `FullParams` - Parameters for transcription
- `SamplingStrategy` - Beam search or greedy sampling

For FFI bindings, use:
```toml
[dependencies]
whisper-rs-sys = { path = "${HANDSFREEVC_DEV_HOME}/whisper-rs/sys" }
```

The `whisper-rs-sys` crate provides direct FFI bindings to whisper.cpp C API.

## Backtrack Branch Features

This build includes backtrack branch features:
- Token candidates API
- Skip encode for interactive transcription
- Forced tokens for re-decoding

See examples in the repository for usage.
EOF

# Copy sys bindings header for reference
if [ -f "$REPO_ROOT/sys/src/bindings.rs" ]; then
    cp -v "$REPO_ROOT/sys/src/bindings.rs" "${INSTALL_ROOT}/include/"
fi

# Create version file
echo ""
echo "==> Creating version file..."
WHISPER_RS_VERSION=$(grep '^version = ' "$REPO_ROOT/Cargo.toml" | head -1 | cut -d'"' -f2)
WHISPER_RS_SYS_VERSION=$(grep '^version = ' "$REPO_ROOT/sys/Cargo.toml" | head -1 | cut -d'"' -f2)
GIT_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_DATE=$(date -u +"%Y-%m-%d %H:%M:%S UTC")

cat > "${INSTALL_ROOT}/VERSION" << EOF
whisper-rs version: ${WHISPER_RS_VERSION}
whisper-rs-sys version: ${WHISPER_RS_SYS_VERSION}
Git commit: ${GIT_COMMIT}
Branch: $(git branch --show-current 2>/dev/null || echo "unknown")
Build date: ${BUILD_DATE}
Backtrack features: enabled
EOF

cat "${INSTALL_ROOT}/VERSION"

# Create README
echo ""
echo "==> Creating README..."
cat > "${INSTALL_ROOT}/README.md" << 'EOF'
# whisper-rs Pre-built Libraries

Rust bindings to whisper.cpp (backtrack branch) for Linux and Android platforms.

This is a complete Cargo package with prebuilt .rlib files for faster builds.

## Directory Structure

```
whisper-rs/
├── Cargo.toml           # Main crate manifest
├── src/                 # Rust source files
├── sys/                 # whisper-rs-sys subcrate
│   ├── Cargo.toml
│   ├── build.rs
│   ├── wrapper.h
│   └── src/
│       ├── lib.rs
│       └── bindings.rs  # FFI bindings
├── include/             # Documentation
│   ├── README.md
│   └── bindings.rs      # FFI bindings reference (copy)
├── linux-x86_64/        # Prebuilt Linux libraries
│   └── libwhisper_rs.rlib
├── android/             # Prebuilt Android libraries by ABI
│   ├── arm64-v8a/
│   ├── armeabi-v7a/
│   ├── x86_64/
│   └── x86/
├── README.md
└── VERSION
```

## Dependencies

whisper-rs requires prebuilt whisper.cpp libraries. Ensure they are available at:
```
$HANDSFREEVC_DEV_HOME/whisper.cpp/
```

Set the environment variable:
```bash
export HANDSFREEVC_DEV_HOME=/path/to/dev/home
```

## Usage in Rust Projects

Add to your `Cargo.toml`:

```toml
[dependencies]
whisper-rs = { path = "/path/to/whisper-rs" }
# Or using environment variable:
# whisper-rs = { path = "${HANDSFREEVC_DEV_HOME}/whisper-rs" }
```

This will use the prebuilt .rlib files automatically based on your target platform.
The package includes all source files, so Cargo can:
- Use prebuilt libraries when available
- Recompile if needed for different configurations
- Support both Linux and Android targets seamlessly

### Environment Setup

Ensure `HANDSFREEVC_DEV_HOME` is set and points to the directory containing
both `whisper.cpp/` and `whisper-rs/`:

```bash
export HANDSFREEVC_DEV_HOME=/path/to/dev/home
```

## Backtrack Branch Features

This build includes backtrack branch features for interactive transcription:

- **Token Candidates**: Access top candidate tokens with probabilities
  - `set_capture_top_candidates()`, `set_n_top_candidates()`
  - `WhisperToken::n_top_candidates()`, `get_top_candidate()`

- **Skip Encode**: Reuse encoded audio for multiple decode passes
  - `set_skip_encode(true)` for subsequent transcriptions

- **Forced Tokens**: Force specific tokens and re-decode
  - `set_forced_tokens()` for exploring alternatives

See the `whsh` example in the repository for interactive usage.

## Rebuilding

To rebuild from source:
```bash
cd /path/to/whisper-rs
./build-and-install.sh --all
```

Options:
- `--with-android` or `--all`: Build for Android targets (requires installed targets)
- No args: Build only for native Linux x86_64

## Version Information

See `VERSION` file for build details.
EOF

echo ""
echo "==> Installation complete!"
echo ""
echo "    Installed to: ${INSTALL_ROOT}"
echo ""
echo "    To use in your project, add to Cargo.toml:"
echo "    [dependencies]"
echo "    whisper-rs = { path = \"${INSTALL_ROOT}\" }"
echo ""
echo "    Or set HANDSFREEVC_DEV_HOME environment variable:"
echo "    export HANDSFREEVC_DEV_HOME=${HANDSFREEVC_DEV_HOME}"
echo ""
