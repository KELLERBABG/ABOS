use abos_common::error::{Error, Result};
use std::collections::HashSet;

/// Regular LDPC code encoder/decoder
pub struct LDPCCode {
    pub parity_check: Vec<Vec<u8>>,  // H matrix (m x n)
    pub generator: Vec<Vec<u8>>,     // G matrix (k x n)
    pub k: usize,                    // Message length
    pub n: usize,                    // Codeword length
    m: usize,                        // Number of parity checks
    col_to_rows: Vec<Vec<usize>>,    // Variable node to check node connections
}

impl LDPCCode {
    /// Construct a regular (n, k) LDPC code
    /// Creates a simple parity check matrix with 3 ones per column
    pub fn new(k: usize, n: usize) -> Self {
        let m = n - k;
        let mut parity_check = vec![vec![0u8; n]; m];

        // Build a simple regular parity check matrix (dv=3 per column)
        let mut rng = 0usize;
        for col in 0..n {
            let mut placed = HashSet::new();
            while placed.len() < 3 {
                let row = (rng % m + col) % m;
                if placed.insert(row) {
                    parity_check[row][col] = 1;
                }
                rng += 1;
            }
        }

        // Build connections for belief propagation
        let mut col_to_rows = vec![Vec::new(); n];
        for row in 0..m {
            for col in 0..n {
                if parity_check[row][col] == 1 {
                    col_to_rows[col].push(row);
                }
            }
        }

        // Build systematic generator matrix [I | P]
        let mut generator = vec![vec![0u8; n]; k];
        for i in 0..k {
            generator[i][i] = 1; // Identity part
        }
        // P part from parity check via Gaussian elimination (simplified)
        for row in 0..m.min(k) {
            for col in 0..n {
                if col >= k {
                    generator[row][col] = parity_check[row][col - k];
                }
            }
        }

        Self {
            parity_check,
            generator,
            k,
            n,
            m,
            col_to_rows,
        }
    }

    /// Encode data with the generator matrix
    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut codeword = vec![0u8; self.n];

        // Copy data into systematic positions
        for i in 0..self.k.min(data.len() * 8) {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            if byte_idx < data.len() {
                codeword[i] = (data[byte_idx] >> bit_idx) & 0x01;
            }
        }

        // Compute parity bits
        for col in self.k..self.n {
            let mut parity = 0u8;
            for row in 0..self.m {
                if self.parity_check[row][col] == 1 {
                    for var in 0..self.k {
                        if self.parity_check[row][var] == 1 && codeword[var] == 1 {
                            parity ^= 1;
                        }
                    }
                }
            }
            codeword[col] = parity;
        }

        // Pack bits back to bytes
        let byte_len = (self.n + 7) / 8;
        let mut result = vec![0u8; byte_len];
        for i in 0..self.n {
            if codeword[i] == 1 {
                result[i / 8] |= 1 << (i % 8);
            }
        }
        result
    }

    /// Decode with belief propagation (sum-product algorithm)
    pub fn decode(&self, llrs: &[f64], max_iter: usize) -> Result<Vec<u8>> {
        if llrs.len() < self.n {
            return Err(Error::FecError("LLR vector too short".into()));
        }

        let mut var_to_check = vec![vec![0.0f64; self.m]; self.n];
        let mut check_to_var = vec![vec![0.0f64; self.n]; self.m];
        let var_llrs = llrs[..self.n].to_vec();

        for _iter in 0..max_iter {
            // Variable node to check node
            for var in 0..self.n {
                for &row in &self.col_to_rows[var] {
                    let mut sum = var_llrs[var];
                    for &other_row in &self.col_to_rows[var] {
                        if other_row != row {
                            sum += check_to_var[other_row][var];
                        }
                    }
                    var_to_check[var][row] = sum;
                }
            }

            // Check node to variable node (tanh rule)
            for row in 0..self.m {
                for var in 0..self.n {
                    if self.parity_check[row][var] == 1 {
                        let mut product = 1.0f64;
                        for other_var in 0..self.n {
                            if other_var != var && self.parity_check[row][other_var] == 1 {
                                product *= (var_to_check[other_var][row] / 2.0).tanh();
                            }
                        }
                        // Clamp product to avoid numerical issues
                        product = product.clamp(-0.9999, 0.9999);
                        check_to_var[row][var] = 2.0 * product.atanh();
                    }
                }
            }

            // Tentative decoding
            let mut decisions = vec![0u8; self.n];
            for var in 0..self.n {
                let mut sum = var_llrs[var];
                for &row in &self.col_to_rows[var] {
                    sum += check_to_var[row][var];
                }
                decisions[var] = if sum > 0.0 { 1 } else { 0 };
            }

            // Check if valid codeword
            let mut valid = true;
            for row in 0..self.m {
                let mut check = 0u8;
                for col in 0..self.n {
                    if self.parity_check[row][col] == 1 {
                        check ^= decisions[col];
                    }
                }
                if check != 0 {
                    valid = false;
                    break;
                }
            }

            if valid {
                let byte_len = (self.k + 7) / 8;
                let mut result = vec![0u8; byte_len];
                for i in 0..self.k {
                    if decisions[i] == 1 {
                        result[i / 8] |= 1 << (i % 8);
                    }
                }
                return Ok(result);
            }
        }

        Err(Error::FecError("LDPC decoding failed to converge".into()))
    }
}