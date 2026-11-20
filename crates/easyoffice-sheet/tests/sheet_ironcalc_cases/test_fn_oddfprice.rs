use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

// =ODDFPRICE("3/1/2020","3/1/2030","1/15/2020","3/1/2021",7.5%,0%,120,1,2)

#[test]
fn test_oddfprice_basis_2_zero_yield() {
    let mut model = new_empty_model();
    // Too few args
    model._set("A1", "=DATE(2020,3,1)"); // Settlement: 1/March/2020
    model._set("A2", "=DATE(2030,3,1)"); // Maturity: 1/March/2030
    model._set("A3", "=DATE(2020,1,15)"); // Issue: 15/Jan/2020
    model._set("A4", "=DATE(2021,3,1)"); // 1/March/2021
    model._set("A5", "7.5%"); // Rate: 7.5%
    model._set("A6", "0%"); // Yield: 0%
    model._set("A7", "120"); // Redemption
    model._set("A8", "1"); // Frequency: 1 (Annual)
    model._set("A9", "2"); // Basis: 2 (Actual/360)
    model._set("C3", "=ODDFPRICE(A1,A2,A3,A4,A5,A6,A7,A8,A9)");
    model.evaluate();
    assert_eq!(model._get_text("C3"), "195");
}
