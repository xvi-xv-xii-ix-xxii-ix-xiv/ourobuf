# 🌀 Ourobuf

[![Crates.io](https://img.shields.io/crates/v/ourobuf.svg)](https://crates.io/crates/ourobuf)
[![Docs.rs](https://docs.rs/ourobuf/badge.svg)](https://docs.rs/ourobuf/0.1.1/ourobuf/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)]()

Thread-safe circular buffer for embedded systems and high-performance applications

## Features ✨

- 🛡️ 100% Safe Rust with `no_std` support
- ⚡ Constant-time O(1) operations
- 🔒 Spinlock-based thread safety
- 📏 Configurable size via const generics
- 🔄 Heapless Vec integration
- 🔋 Zero allocations
- 🧼 Memory zeroization

## Designed For 🎯

- Real-time data streaming (sensors, network packets)
- Interrupt-safe logging
- Lock-free inter-thread communication
- Embedded systems (no_std)
- High-throughput data pipelines

## Quick Start 🚀

```rust
use ourobuf::OuroBuffer;

fn main() -> Result<(), ourobuf::OuroBufferError> {
    // Create 256-byte buffer
    let buf = OuroBuffer::<256>::new();

    // Write data
    buf.push(b"Hello")?;

    // Read data
    let mut output = [0u8; 5];
    let read = buf.pop(&mut output);

    assert_eq!(&output[..read], b"Hello");
    Ok(())
}
```
