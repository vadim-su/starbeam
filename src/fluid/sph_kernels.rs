use std::f32::consts::PI;

/// Poly6 kernel -- used for density estimation (2D).
pub fn poly6(r: f32, h: f32) -> f32 {
    if r > h {
        return 0.0;
    }
    let h2 = h * h;
    let r2 = r * r;
    let diff = h2 - r2;
    let coeff = 4.0 / (PI * h.powi(8));
    coeff * diff.powi(3)
}

/// Spiky kernel gradient magnitude -- used for pressure forces (2D).
pub fn spiky_gradient(r: f32, h: f32) -> f32 {
    if r > h || r < 1e-6 {
        return 0.0;
    }
    let diff = h - r;
    let coeff = -10.0 / (PI * h.powi(5));
    coeff * diff * diff
}

/// Viscosity kernel Laplacian -- used for viscosity forces (2D).
pub fn viscosity_laplacian(r: f32, h: f32) -> f32 {
    if r > h {
        return 0.0;
    }
    let coeff = 40.0 / (PI * h.powi(5));
    coeff * (h - r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poly6_peak_at_zero() {
        let h = 10.0;
        let val = poly6(0.0, h);
        assert!(val > 0.0, "Poly6 at r=0 should be positive");
    }

    #[test]
    fn poly6_zero_at_boundary() {
        let h = 10.0;
        let val = poly6(h, h);
        assert!(val.abs() < 1e-6, "Poly6 at r=h should be ~0");
    }

    #[test]
    fn poly6_zero_beyond_boundary() {
        let h = 10.0;
        let val = poly6(h + 1.0, h);
        assert_eq!(val, 0.0);
    }

    #[test]
    fn spiky_grad_zero_at_zero() {
        let h = 10.0;
        let val = spiky_gradient(0.0, h);
        assert_eq!(val, 0.0, "Spiky gradient at r=0 should be 0");
    }

    #[test]
    fn spiky_grad_negative_inside() {
        let h = 10.0;
        let val = spiky_gradient(5.0, h);
        assert!(val < 0.0, "Spiky gradient should be negative (repulsive)");
    }

    #[test]
    fn viscosity_laplacian_positive_inside() {
        let h = 10.0;
        let val = viscosity_laplacian(5.0, h);
        assert!(val > 0.0, "Viscosity laplacian should be positive inside h");
    }

    #[test]
    fn viscosity_laplacian_zero_beyond() {
        let h = 10.0;
        let val = viscosity_laplacian(h + 1.0, h);
        assert_eq!(val, 0.0);
    }
}
