#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_mathlib() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        // Basic math functions
        assert_eq!(ws.expr_ty("math.min(1, 2)"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("math.max(1, 2.0)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.abs(-5)"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("math.abs(-5.5)"), ws.ty("number"));

        // Rounding functions
        assert_eq!(ws.expr_ty("math.floor(2.5)"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("math.ceil(2.5)"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("math.modf(2.5)"), ws.ty("integer"));

        // Trigonometric functions
        assert_eq!(ws.expr_ty("math.sin(1)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.cos(1)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.tan(1)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.asin(0.5)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.acos(0.5)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.atan(0.5)"), ws.ty("number"));

        // Other math functions
        assert_eq!(ws.expr_ty("math.sqrt(16)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.exp(1)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.log(10)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.rad(180)"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.deg(3.14)"), ws.ty("number"));

        // Random functions
        assert_eq!(ws.expr_ty("math.random()"), ws.ty("number"));
        assert_eq!(ws.expr_ty("math.random(10)"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("math.random(1, 10)"), ws.ty("integer"));
    }
}
