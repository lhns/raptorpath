//! Streaming FEC interface — sends source symbols immediately,
//! then streams repair symbols based on controller demand.

use super::traits::{EncodingParams, FecBackend, FecEncoder, WireSymbol};

/// A FEC stream that first yields all source symbols, then generates
/// repair symbols on demand from the controller.
pub struct FecStream {
    encoder: Box<dyn FecEncoder>,
    source_emitted: bool,
}

impl FecStream {
    pub fn new(data: &[u8], params: EncodingParams, backend: FecBackend) -> Self {
        Self {
            encoder: backend.create_encoder(data, params),
            source_emitted: false,
        }
    }

    /// Get source symbols (call once, first).
    pub fn take_source_symbols(&mut self) -> Vec<WireSymbol> {
        self.source_emitted = true;
        self.encoder.source_symbols()
    }

    /// Generate additional repair symbols on demand.
    /// Can be called multiple times — fountain codes generate unlimited repair.
    pub fn generate_repair(&self, count: u32) -> Vec<WireSymbol> {
        self.encoder.repair_symbols(count)
    }
}
