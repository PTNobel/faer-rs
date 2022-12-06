use assert2::assert as fancy_assert;
use dyn_stack::DynStack;
use faer_core::{
    householder::apply_block_householder_sequence_on_the_left, solve, ComplexField, Conj, MatMut,
    MatRef, Parallelism,
};
use reborrow::*;

pub fn solve_in_place<T: ComplexField>(
    qr_factors: MatRef<'_, T>,
    householder_factor: MatRef<'_, T>,
    conj_lhs: Conj,
    rhs: MatMut<'_, T>,
    conj_rhs: Conj,
    parallelism: Parallelism,
    stack: DynStack<'_>,
) {
    // conjᵃ(H₀ × ... × Hₖ₋₁ × R) X = conjᵇ(B)
    // X = conjᵃ(R)⁻¹ × conjᵃ(Hₖ₋₁) × ... × conjᵃ(H₀) × conjᵇ(B)
    fancy_assert!(qr_factors.nrows() == qr_factors.ncols());
    let size = qr_factors.nrows();
    let blocksize = householder_factor.nrows();
    fancy_assert!((householder_factor.nrows(), householder_factor.ncols()) == (blocksize, size));
    fancy_assert!(rhs.nrows() == qr_factors.nrows());

    let mut rhs = rhs;
    let mut stack = stack;
    apply_block_householder_sequence_on_the_left(
        qr_factors,
        householder_factor,
        conj_lhs,
        rhs.rb_mut(),
        conj_rhs,
        true,
        parallelism,
        stack.rb_mut(),
    );

    solve::solve_upper_triangular_in_place(qr_factors, conj_lhs, rhs, Conj::No, parallelism);
}

pub fn solve_transpose_in_place<T: ComplexField>(
    qr_factors: MatRef<'_, T>,
    householder_factor: MatRef<'_, T>,
    conj_lhs: Conj,
    rhs: MatMut<'_, T>,
    conj_rhs: Conj,
    parallelism: Parallelism,
    stack: DynStack<'_>,
) {
    // conjᵃ(H₀ × ... × Hₖ₋₁ × R)ᵀ X = conjᵇ(B)
    // conjᵃ(Rᵀ × Hₖ₋₁ᵀ × ... × H₀ᵀ) X = conjᵇ(B)
    // X = conj(conjᵃ(H₀)) × ... × conj(conjᵃ(Hₖ₋₁)) × (conjᵃ(R)ᵀ)⁻¹ × conjᵇ(B)
    fancy_assert!(qr_factors.nrows() == qr_factors.ncols());
    let size = qr_factors.nrows();
    let blocksize = householder_factor.nrows();
    fancy_assert!((householder_factor.nrows(), householder_factor.ncols()) == (blocksize, size));
    fancy_assert!(rhs.nrows() == qr_factors.nrows());

    let mut rhs = rhs;
    let mut stack = stack;

    solve::solve_lower_triangular_in_place(
        qr_factors.transpose(),
        conj_lhs,
        rhs.rb_mut(),
        conj_rhs,
        parallelism,
    );
    apply_block_householder_sequence_on_the_left(
        qr_factors,
        householder_factor,
        conj_lhs.compose(Conj::Yes),
        rhs.rb_mut(),
        Conj::No,
        false,
        parallelism,
        stack.rb_mut(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use faer_core::{c32, c64, mul::matmul, Mat};
    use rand::random;

    use crate::no_pivoting::compute::{qr_in_place, recommended_blocksize};

    macro_rules! placeholder_stack {
        () => {
            ::dyn_stack::DynStack::new(&mut ::dyn_stack::GlobalMemBuffer::new(
                ::dyn_stack::StackReq::new::<T>(1024 * 1024),
            ))
        };
    }

    fn test_solve_in_place<T: ComplexField>(mut random: impl FnMut() -> T, epsilon: T::Real) {
        let n = 32;
        let k = 6;

        let a = Mat::with_dims(|_, _| random(), n, n);
        let rhs = Mat::with_dims(|_, _| random(), n, k);

        let mut qr = a.clone();
        let blocksize = recommended_blocksize::<f64>(n, n);
        let mut householder = Mat::with_dims(|_, _| T::zero(), blocksize, n);
        qr_in_place(
            qr.as_mut(),
            householder.as_mut(),
            Parallelism::None,
            placeholder_stack!(),
            Default::default(),
        );

        let qr = qr.as_ref();

        for conj_lhs in [Conj::No, Conj::Yes] {
            for conj_rhs in [Conj::No, Conj::Yes] {
                let mut sol = rhs.clone();
                solve_in_place(
                    qr,
                    householder.as_ref(),
                    conj_lhs,
                    sol.as_mut(),
                    conj_rhs,
                    Parallelism::None,
                    placeholder_stack!(),
                );

                let mut rhs_reconstructed = rhs.clone();
                matmul(
                    rhs_reconstructed.as_mut(),
                    Conj::No,
                    a.as_ref(),
                    conj_lhs,
                    sol.as_ref(),
                    Conj::No,
                    None,
                    T::one(),
                    Parallelism::None,
                );

                for j in 0..k {
                    for i in 0..n {
                        let target = match conj_rhs {
                            Conj::No => rhs[(i, j)],
                            Conj::Yes => rhs[(i, j)].conj(),
                        };

                        fancy_assert!((rhs_reconstructed[(i, j)] - target).abs() < epsilon)
                    }
                }
            }
        }
    }

    fn test_solve_transpose_in_place<T: ComplexField>(
        mut random: impl FnMut() -> T,
        epsilon: T::Real,
    ) {
        let n = 32;
        let k = 6;

        let a = Mat::with_dims(|_, _| random(), n, n);
        let rhs = Mat::with_dims(|_, _| random(), n, k);

        let mut qr = a.clone();
        let blocksize = recommended_blocksize::<f64>(n, n);
        let mut householder = Mat::with_dims(|_, _| T::zero(), blocksize, n);
        qr_in_place(
            qr.as_mut(),
            householder.as_mut(),
            Parallelism::None,
            placeholder_stack!(),
            Default::default(),
        );

        let qr = qr.as_ref();

        for conj_lhs in [Conj::No, Conj::Yes] {
            for conj_rhs in [Conj::No, Conj::Yes] {
                let mut sol = rhs.clone();
                solve_transpose_in_place(
                    qr,
                    householder.as_ref(),
                    conj_lhs,
                    sol.as_mut(),
                    conj_rhs,
                    Parallelism::None,
                    placeholder_stack!(),
                );

                let mut rhs_reconstructed = rhs.clone();
                matmul(
                    rhs_reconstructed.as_mut(),
                    Conj::No,
                    a.as_ref().transpose(),
                    conj_lhs,
                    sol.as_ref(),
                    Conj::No,
                    None,
                    T::one(),
                    Parallelism::None,
                );

                for j in 0..k {
                    for i in 0..n {
                        let target = match conj_rhs {
                            Conj::No => rhs[(i, j)],
                            Conj::Yes => rhs[(i, j)].conj(),
                        };

                        fancy_assert!((rhs_reconstructed[(i, j)] - target).abs() < epsilon)
                    }
                }
            }
        }
    }

    #[test]
    fn test_solve_in_place_f64() {
        test_solve_in_place(random::<f64>, 1e-6);
    }
    #[test]
    fn test_solve_in_place_f32() {
        test_solve_in_place(random::<f32>, 1e-1);
    }

    #[test]
    fn test_solve_in_place_c64() {
        test_solve_in_place(|| c64::new(random(), random()), 1e-6);
    }

    #[test]
    fn test_solve_in_place_c32() {
        test_solve_in_place(|| c32::new(random(), random()), 1e-1);
    }

    #[test]
    fn test_solve_transpose_in_place_f64() {
        test_solve_transpose_in_place(random::<f64>, 1e-6);
    }

    #[test]
    fn test_solve_transpose_in_place_f32() {
        test_solve_transpose_in_place(random::<f32>, 1e-1);
    }

    #[test]
    fn test_solve_transpose_in_place_c64() {
        test_solve_transpose_in_place(|| c64::new(random(), random()), 1e-6);
    }

    #[test]
    fn test_solve_transpose_in_place_c32() {
        test_solve_transpose_in_place(|| c32::new(random(), random()), 1e-1);
    }
}