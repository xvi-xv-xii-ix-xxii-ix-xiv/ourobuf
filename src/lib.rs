#![no_std]
//! Thread-safe circular buffer implementation for embedded systems and high-performance applications
//!
//! ## Key Features
//! - 🛡️ 100% Safe Rust with `no_std` support
//! - ⚡ Constant-time O(1) operations
//! - 🔒 Thread-safe using spinlock-based Mutex
//! - 📏 Configurable size via const generics
//! - 🔄 Seamless integration with [`heapless::Vec`](https://docs.rs/heapless)
//!
//! ## Use Cases
//! - Real-time data streaming (sensors, network packets)
//! - Interrupt-safe logging in embedded systems
//! - Multi-producer/single-consumer (MPSC) communication
//! - Lock-free communication between threads/cores
//!
//! ## Example
//! ```rust
//! use ourobuf::OuroBuffer;
//!
//! let buf = OuroBuffer::<256>::new();
//!
//! // Write data from multiple threads
//! buf.push(b"Hello").unwrap();
//!
//! // Read into mutable slice
//! let mut output = [0u8; 5];
//! let read = buf.pop(&mut output);
//! assert_eq!(&output[..read], b"Hello");
//! ```

use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};
use spin::Mutex;

/// Error types for buffer operations
#[derive(Debug, PartialEq)]
pub enum OuroBufferError {
    /// Occurs when trying to push more data than available space
    ///
    /// # Example
    /// ```
    /// use ourobuf::{OuroBuffer, OuroBufferError};
    ///
    /// let buf = OuroBuffer::<4>::new();
    /// assert_eq!(buf.push(&[1, 2, 3, 4, 5]), Err(OuroBufferError::BufferOverflow));
    /// ```
    BufferOverflow,
}

impl fmt::Display for OuroBufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferOverflow => write!(
                f,
                "Buffer overflow: attempted to write beyond buffer capacity"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for OuroBufferError {}

/// Internal buffer state protected by spinlock
struct BufferState<const N: usize> {
    buffer: [u8; N],
    write_pos: usize,
    read_pos: usize,
    count: usize,
}

/// Thread-safe circular buffer implementation
///
/// # Generic Parameters
/// - `N`: Buffer capacity in bytes (must be power of two for best performance)
///
/// # Implementation Details
/// - Uses double-check locking pattern for optimal performance
/// - Atomic counters for lock-free size checks
/// - Zero-cost initialization through const generics
pub struct OuroBuffer<const N: usize> {
    /// Protected internal state (spinlock-guarded)
    state: Mutex<BufferState<N>>,
    /// Atomic counter for lock-free size queries
    atomic_count: AtomicUsize,
}

impl<const N: usize> OuroBuffer<N> {
    /// Creates a new empty buffer with zero-initialized storage
    ///
    /// # Examples
    /// ```
    /// use ourobuf::OuroBuffer;
    ///
    /// // Create 256-byte buffer
    /// let buf = OuroBuffer::<256>::new();
    /// assert!(buf.is_empty());
    /// ```
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(BufferState {
                buffer: [0; N],
                write_pos: 0,
                read_pos: 0,
                count: 0,
            }),
            atomic_count: AtomicUsize::new(0),
        }
    }

    /// Appends data to the buffer (thread-safe)
    ///
    /// # Parameters
    /// - `data`: Byte slice to write
    ///
    /// # Errors
    /// Returns `OuroBufferError::BufferOverflow` if insufficient space
    ///
    /// # Performance
    /// - Worst-case: 2 copy operations
    /// - Atomic operations: 2 loads, 1 store
    pub fn push(&self, data: &[u8]) -> Result<(), OuroBufferError> {
        let data_len = data.len();
        let available = N - self.atomic_count.load(Ordering::Relaxed);

        // Fast-path check without lock
        if data_len > available {
            return Err(OuroBufferError::BufferOverflow);
        }

        let mut state = self.state.lock();
        let actual_available = N - state.count;

        // Double-check after acquiring lock
        if data_len > actual_available {
            return Err(OuroBufferError::BufferOverflow);
        }

        // Split data into contiguous chunks
        let write_pos = state.write_pos;
        let first_chunk = core::cmp::min(data_len, N - write_pos);
        let (first, second) = data.split_at(first_chunk);

        // Copy data to buffer
        state.buffer[write_pos..write_pos + first_chunk].copy_from_slice(first);
        state.buffer[..second.len()].copy_from_slice(second);

        // Update state
        state.write_pos = (write_pos + data_len) % N;
        state.count += data_len;
        self.atomic_count.store(state.count, Ordering::Release);

        Ok(())
    }

    /// Reads data from buffer into mutable slice (thread-safe)
    ///
    /// # Parameters
    /// - `output`: Target slice for read data
    ///
    /// # Returns
    /// Number of bytes actually read
    ///
    /// # Performance
    /// - Worst-case: 2 copy operations
    /// - Atomic operations: 2 loads, 1 store
    pub fn pop(&self, output: &mut [u8]) -> usize {
        // Lock-free early exit
        let atomic_count = self.atomic_count.load(Ordering::Relaxed);
        if atomic_count == 0 {
            return 0;
        }

        let mut state = self.state.lock();
        let to_read = core::cmp::min(output.len(), state.count);
        if to_read == 0 {
            return 0;
        }

        // Split output buffer
        let read_pos = state.read_pos;
        let first_chunk = core::cmp::min(to_read, N - read_pos);
        let (first, second) = output[..to_read].split_at_mut(first_chunk);

        // Copy data from buffer
        first.copy_from_slice(&state.buffer[read_pos..read_pos + first_chunk]);
        second.copy_from_slice(&state.buffer[..second.len()]);

        // Update state
        state.read_pos = (read_pos + to_read) % N;
        state.count -= to_read;
        self.atomic_count.store(state.count, Ordering::Release);

        to_read
    }

    /// Returns current number of bytes in buffer (lock-free)
    ///
    /// # Performance
    /// - Single atomic load with acquire ordering
    pub fn len(&self) -> usize {
        self.atomic_count.load(Ordering::Acquire)
    }

    /// Checks if buffer is empty (lock-free)
    ///
    /// Equivalent to `self.len() == 0`
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns available free space in bytes (lock-free)
    ///
    /// Calculated as `capacity() - len()`
    pub fn available_space(&self) -> usize {
        N - self.len()
    }

    /// Resets buffer to empty state and zeros memory
    ///
    /// # Security Note
    /// Zeroization helps prevent sensitive data leakage
    pub fn clear(&self) {
        let mut state = self.state.lock();
        state.write_pos = 0;
        state.read_pos = 0;
        state.count = 0;
        state.buffer.iter_mut().for_each(|x| *x = 0);
        self.atomic_count.store(0, Ordering::Release);
    }
}

// Clone implementation creates snapshot of current state
impl<const N: usize> Clone for OuroBuffer<N> {
    /// Creates a copy of current buffer state
    ///
    /// # Performance
    /// - Full buffer copy (O(N) complexity)
    fn clone(&self) -> Self {
        let state = self.state.lock();
        Self {
            state: Mutex::new(BufferState {
                buffer: state.buffer,
                write_pos: state.write_pos,
                read_pos: state.read_pos,
                count: state.count,
            }),
            atomic_count: AtomicUsize::new(state.count),
        }
    }
}

/// heapless::Vec integration (enabled via "heapless" feature)
#[cfg(feature = "heapless")]
impl<const N: usize> OuroBuffer<N> {
    /// Pushes data from [`heapless::Vec`]  
    ///
    /// # Parameters
    /// - `data`: Vec with capacity `V`
    ///
    /// # Example
    /// ```
    /// # use ourobuf::OuroBuffer;
    /// # use heapless::Vec;
    /// let buf = OuroBuffer::<256>::new();
    /// let mut vec = Vec::<u8, 32>::new();
    /// vec.extend_from_slice(b"data").unwrap();
    /// buf.push_heapless(&vec).unwrap();
    /// ```
    pub fn push_heapless<const V: usize>(
        &self,
        data: &heapless::Vec<u8, V>,
    ) -> Result<(), OuroBufferError> {
        self.push(data.as_slice())
    }

    /// Pops data into new [`heapless::Vec`]
    ///
    /// # Parameters
    /// - `count`: Maximum bytes to pop
    ///
    /// # Returns
    /// Vec containing up to `count` bytes (actual size ≤ min(count, V, N))
    pub fn pop_heapless<const V: usize>(&self, count: usize) -> heapless::Vec<u8, V> {
        let mut vec = heapless::Vec::new();
        let mut temp = [0u8; N];
        let to_read = count.min(V).min(N);
        let read = self.pop(&mut temp[..to_read]);
        let _ = vec.extend_from_slice(&temp[..read]);
        vec
    }
}

// Debug implementation shows buffer utilization
impl<const N: usize> fmt::Debug for OuroBuffer<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OuroBuffer<{}>[{}B used, {}B free]",
            N,
            self.len(),
            self.available_space()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_operations() {
        let buf = OuroBuffer::<8>::new();
        assert_eq!(buf.len(), 0);

        assert!(buf.push(&[1, 2, 3]).is_ok());
        assert_eq!(buf.len(), 3);

        let mut output = [0u8; 3];
        assert_eq!(buf.pop(&mut output), 3);
        assert_eq!(output, [1, 2, 3]);
        assert!(buf.is_empty());
    }

    #[cfg(feature = "heapless")]
    #[test]
    fn heapless_integration() {
        use heapless::Vec;

        let buf = OuroBuffer::<16>::new();
        let mut vec = Vec::<u8, 4>::new();
        vec.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();

        buf.push_heapless(&vec).unwrap();
        let result = buf.pop_heapless::<8>(4);
        assert_eq!(result.as_slice(), &[0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
