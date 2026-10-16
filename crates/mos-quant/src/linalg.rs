//! Small dense linear algebra: just what regression and correlated simulation need.

use mos_core::error::{Error, Result};

pub type Matrix = Vec<Vec<f64>>;

fn square(what: &'static str, a: &Matrix) -> Result<usize> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Err(Error::invalid(what, "matrix must be square and non-empty".to_string()));
    }
    if a.iter().flatten().any(|x| !x.is_finite()) {
        return Err(Error::invalid(what, "matrix has non-finite entries".to_string()));
    }
    Ok(n)
}

/// Lower-triangular `L` with `L Lᵀ = a`. Fails if `a` is not symmetric positive definite.
pub fn cholesky(a: &Matrix) -> Result<Matrix> {
    let n = square("cholesky", a)?;
    let scale = a.iter().enumerate().map(|(i, row)| row[i].abs()).fold(0.0, f64::max).max(1.0);
    for i in 0..n {
        for j in 0..i {
            if (a[i][j] - a[j][i]).abs() > 1e-9 * scale {
                return Err(Error::invalid("cholesky", format!("matrix is not symmetric at ({i}, {j})")));
            }
        }
    }
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let sum: f64 = (0..j).map(|k| l[i][k] * l[j][k]).sum();
            if i == j {
                let d = a[i][i] - sum;
                if d <= 1e-12 * scale {
                    return Err(Error::invalid("cholesky", format!("matrix is not positive definite (pivot {i} is {d:e})")));
                }
                l[i][j] = d.sqrt();
            } else {
                l[i][j] = (a[i][j] - sum) / l[j][j];
            }
        }
    }
    Ok(l)
}

/// Solves `a x = b` for symmetric positive definite `a`.
pub fn solve_spd(a: &Matrix, b: &[f64]) -> Result<Vec<f64>> {
    let l = cholesky(a)?;
    let n = l.len();
    if b.len() != n {
        return Err(Error::invalid("solve", format!("right-hand side has {} entries for a {n}x{n} system", b.len())));
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let sum: f64 = (0..i).map(|k| l[i][k] * y[k]).sum();
        y[i] = (b[i] - sum) / l[i][i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let sum: f64 = (i + 1..n).map(|k| l[k][i] * x[k]).sum();
        x[i] = (y[i] - sum) / l[i][i];
    }
    Ok(x)
}

pub fn mat_vec(a: &Matrix, x: &[f64]) -> Result<Vec<f64>> {
    if a.iter().any(|row| row.len() != x.len()) {
        return Err(Error::invalid("matrix product", "dimension mismatch".to_string()));
    }
    Ok(a.iter().map(|row| row.iter().zip(x).map(|(a, b)| a * b).sum()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cholesky_reconstructs_and_solves() {
        let a = vec![vec![4.0, 12.0, -16.0], vec![12.0, 37.0, -43.0], vec![-16.0, -43.0, 98.0]];
        let l = cholesky(&a).unwrap();
        assert_eq!(l, vec![vec![2.0, 0.0, 0.0], vec![6.0, 1.0, 0.0], vec![-8.0, 5.0, 3.0]]);
        let x = solve_spd(&a, &[1.0, 2.0, 3.0]).unwrap();
        let back = mat_vec(&a, &x).unwrap();
        for (got, want) in back.iter().zip([1.0, 2.0, 3.0]) {
            assert!((got - want).abs() < 1e-9);
        }
    }

    #[test]
    fn rejects_non_spd() {
        assert!(cholesky(&vec![vec![1.0, 2.0], vec![2.0, 1.0]]).is_err(), "indefinite");
        assert!(cholesky(&vec![vec![1.0, 1.0], vec![1.0, 1.0]]).is_err(), "singular");
        assert!(cholesky(&vec![vec![1.0, 0.5], vec![0.2, 1.0]]).is_err(), "asymmetric");
        assert!(cholesky(&vec![vec![1.0, 0.0]]).is_err(), "not square");
        assert!(solve_spd(&vec![vec![1.0]], &[1.0, 2.0]).is_err());
    }
}
