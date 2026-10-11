//! Strict bounded ASCII matrix reader. Radiance binary interchange is unused.
use anyhow::{ensure, Context, Result};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

const MAX_LINE_BYTES: usize = 2 * 1024 * 1024;
const MAX_MATRIX_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) struct MatrixReader<R: BufRead> {
    reader: R,
    expected: usize,
    consumed: usize,
    line: String,
    pending: Vec<f64>,
    pending_index: usize,
}
impl MatrixReader<BufReader<File>> {
    pub fn open(path: &Path, rows: usize, columns: usize) -> Result<Self> {
        let file = File::open(path).with_context(|| {
            format!(
                "Open generated matrix {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        })?;
        ensure!(
            file.metadata()?.len() <= MAX_MATRIX_BYTES,
            "Engine matrix exceeds 512 MiB resource limit"
        );
        Self::new(BufReader::new(file), rows, columns)
    }
}
impl<R: BufRead> MatrixReader<R> {
    pub fn new(mut reader: R, expected_rows: usize, expected_columns: usize) -> Result<Self> {
        let mut header = BTreeMap::new();
        let mut bytes = 0;
        loop {
            let mut line = String::new();
            let size = (&mut reader).take(65537).read_line(&mut line)?;
            bytes += size;
            ensure!(
                size > 0 && bytes <= 65536,
                "Missing or oversized matrix header"
            );
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some((key, value)) = line.split_once('=') {
                if ["NROWS", "NCOLS", "NCOMP", "FORMAT"].contains(&key) {
                    ensure!(
                        header.insert(key.to_owned(), value.to_owned()).is_none(),
                        "Duplicate matrix header field {key}"
                    );
                }
            }
        }
        ensure!(
            header.get("FORMAT").map(String::as_str) == Some("ascii"),
            "Expected ASCII Radiance matrix"
        );
        ensure!(
            header.get("NCOMP").map(String::as_str) == Some("3"),
            "Expected exactly three visible RGB matrix components"
        );
        let rows: usize = header
            .get("NROWS")
            .context("Matrix NROWS missing")?
            .parse()?;
        let columns: usize = header
            .get("NCOLS")
            .context("Matrix NCOLS missing")?
            .parse()?;
        ensure!(rows == expected_rows && columns == expected_columns && rows > 0 && columns > 0, "Unexpected matrix shape: {rows}x{columns}, expected {expected_rows}x{expected_columns}");
        let expected = rows.checked_mul(columns).context("Matrix size overflow")?;
        Ok(Self {
            reader,
            expected,
            consumed: 0,
            line: String::new(),
            pending: Vec::new(),
            pending_index: 0,
        })
    }
    fn scalar(&mut self) -> Result<Option<f64>> {
        loop {
            if self.pending_index < self.pending.len() {
                let result = self.pending[self.pending_index];
                self.pending_index += 1;
                return Ok(Some(result));
            }
            self.line.clear();
            let size = (&mut self.reader)
                .take(MAX_LINE_BYTES as u64 + 1)
                .read_line(&mut self.line)?;
            ensure!(
                size <= MAX_LINE_BYTES,
                "Matrix line exceeds bounded parser limit"
            );
            if size == 0 {
                return Ok(None);
            }
            self.pending.clear();
            self.pending_index = 0;
            for token in self.line.split_whitespace() {
                let value: f64 = token.parse().context("Malformed matrix number")?;
                ensure!(
                    value.is_finite() && value >= 0.0,
                    "Nonfinite or negative matrix output"
                );
                self.pending.push(value);
            }
        }
    }
    pub fn next_rgb(&mut self) -> Result<Option<[f64; 3]>> {
        if self.consumed == self.expected {
            ensure!(
                self.scalar()?.is_none(),
                "Excess matrix output after declared shape"
            );
            return Ok(None);
        }
        let mut rgb = [0.0; 3];
        for channel in &mut rgb {
            *channel = self.scalar()?.context("Truncated engine matrix")?;
        }
        self.consumed += 1;
        Ok(Some(rgb))
    }
    pub fn finish(mut self) -> Result<()> {
        while self.next_rgb()?.is_some() {}
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn exact_matrix_order_and_shape() {
        let input = "#?RADIANCE\nNROWS=2\nNCOLS=2\nNCOMP=3\nFORMAT=ascii\n\n1 2 3\n4 5 6\n7 8 9\n10 11 12\n";
        let mut reader = MatrixReader::new(Cursor::new(input), 2, 2).unwrap();
        assert_eq!(reader.next_rgb().unwrap(), Some([1., 2., 3.]));
        assert_eq!(reader.next_rgb().unwrap(), Some([4., 5., 6.]));
        assert_eq!(reader.next_rgb().unwrap(), Some([7., 8., 9.]));
        assert_eq!(reader.next_rgb().unwrap(), Some([10., 11., 12.]));
        assert_eq!(reader.next_rgb().unwrap(), None);
    }
    #[test]
    fn rejects_truncated_excess_negative_nonfinite_and_wrong_shape() {
        for numbers in ["1 2", "1 2 3 4", "NaN 2 3", "inf 2 3", "-1 2 3", "x 2 3"] {
            let input = format!("NROWS=1\nNCOLS=1\nNCOMP=3\nFORMAT=ascii\n\n{numbers}\n");
            assert!(MatrixReader::new(Cursor::new(input), 1, 1)
                .unwrap()
                .finish()
                .is_err());
        }
        assert!(MatrixReader::new(
            Cursor::new("NROWS=1\nNCOLS=2\nNCOMP=3\nFORMAT=ascii\n\n"),
            1,
            1
        )
        .is_err());
        assert!(MatrixReader::new(
            Cursor::new("NROWS=1\nNROWS=1\nNCOLS=1\nNCOMP=3\nFORMAT=ascii\n\n"),
            1,
            1
        )
        .is_err());
    }
}
