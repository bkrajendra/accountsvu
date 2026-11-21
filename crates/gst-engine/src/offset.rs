//! Payment of tax (GSTR-3B table 6.1): which input tax credit pays which
//! liability, and what is left to pay in cash.
//!
//! The order follows section 49 and rule 88A of the CGST Rules:
//! * IGST credit is used first against IGST; what remains may go to CGST and
//!   SGST/UTGST in any order.
//! * CGST and SGST/UTGST credit may pay IGST only once IGST credit is fully
//!   used; CGST credit never pays SGST/UTGST, and the reverse.
//! * Cess credit only pays cess.
//! * Reverse-charge tax is always paid in cash.
//!
//! Within those rules the proposal spends the remaining IGST credit where it
//! saves cash (the CGST/SGST shortfall first), so the cash payable is the
//! least possible. It is a proposal: the return is filed on the portal.

use serde::Serialize;

/// Amounts per tax head, in paise. SGST includes UTGST.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heads {
    pub igst: i64,
    pub cgst: i64,
    pub sgst: i64,
    pub cess: i64,
}

impl Heads {
    pub fn total(&self) -> i64 {
        self.igst + self.cgst + self.sgst + self.cess
    }

    pub fn plus(self, o: Heads) -> Heads {
        Heads {
            igst: self.igst + o.igst,
            cgst: self.cgst + o.cgst,
            sgst: self.sgst + o.sgst,
            cess: self.cess + o.cess,
        }
    }

    pub fn minus(self, o: Heads) -> Heads {
        Heads {
            igst: self.igst - o.igst,
            cgst: self.cgst - o.cgst,
            sgst: self.sgst - o.sgst,
            cess: self.cess - o.cess,
        }
    }

    /// Negative heads set to zero.
    pub fn floor(self) -> Heads {
        Heads {
            igst: self.igst.max(0),
            cgst: self.cgst.max(0),
            sgst: self.sgst.max(0),
            cess: self.cess.max(0),
        }
    }
}

/// How one liability head is paid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadPayment {
    pub liability: i64,
    /// Paid from each credit head.
    pub by_credit: Heads,
    pub cash: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OffsetProposal {
    pub igst: HeadPayment,
    pub cgst: HeadPayment,
    pub sgst: HeadPayment,
    pub cess: HeadPayment,
    /// Reverse-charge tax, paid in cash.
    pub reverse_charge: Heads,
    /// Credit left over, carried to the next period.
    pub carry_forward: Heads,
    /// Cash to pay: the forward-charge shortfall plus reverse-charge tax.
    pub cash_total: i64,
}

/// Proposes how `credit` (net ITC available, including any brought forward)
/// pays `liability` (forward-charge output tax), with `reverse_charge` tax
/// paid in cash. Negative inputs are treated as zero.
pub fn propose(liability: Heads, credit: Heads, reverse_charge: Heads) -> OffsetProposal {
    let l = liability.floor();
    let mut c = credit.floor();
    let rc = reverse_charge.floor();
    let mut left = l;
    let mut out = OffsetProposal {
        igst: HeadPayment {
            liability: l.igst,
            ..Default::default()
        },
        cgst: HeadPayment {
            liability: l.cgst,
            ..Default::default()
        },
        sgst: HeadPayment {
            liability: l.sgst,
            ..Default::default()
        },
        cess: HeadPayment {
            liability: l.cess,
            ..Default::default()
        },
        reverse_charge: rc,
        ..Default::default()
    };

    // 1. IGST credit against IGST.
    let x = c.igst.min(left.igst);
    out.igst.by_credit.igst += x;
    c.igst -= x;
    left.igst -= x;

    // 2. Remaining IGST credit against CGST and SGST: first the part their
    //    own credit can't cover, then the rest.
    let short_c = (left.cgst - c.cgst).max(0);
    let short_s = (left.sgst - c.sgst).max(0);
    for (head, short) in [(0, short_c), (1, short_s), (0, i64::MAX), (1, i64::MAX)] {
        let (pay, rem) = if head == 0 {
            (&mut out.cgst, &mut left.cgst)
        } else {
            (&mut out.sgst, &mut left.sgst)
        };
        let x = c.igst.min(*rem).min(short);
        pay.by_credit.igst += x;
        c.igst -= x;
        *rem -= x;
    }

    // 3. CGST and SGST credit against their own heads.
    let x = c.cgst.min(left.cgst);
    out.cgst.by_credit.cgst += x;
    c.cgst -= x;
    left.cgst -= x;
    let x = c.sgst.min(left.sgst);
    out.sgst.by_credit.sgst += x;
    c.sgst -= x;
    left.sgst -= x;

    // 4. IGST still due (IGST credit is used up by now): CGST, then SGST credit.
    let x = c.cgst.min(left.igst);
    out.igst.by_credit.cgst += x;
    c.cgst -= x;
    left.igst -= x;
    let x = c.sgst.min(left.igst);
    out.igst.by_credit.sgst += x;
    c.sgst -= x;
    left.igst -= x;

    // 5. Cess.
    let x = c.cess.min(left.cess);
    out.cess.by_credit.cess += x;
    c.cess -= x;
    left.cess -= x;

    out.igst.cash = left.igst;
    out.cgst.cash = left.cgst;
    out.sgst.cash = left.sgst;
    out.cess.cash = left.cess;
    out.carry_forward = c;
    out.cash_total = left.total() + rc.total();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(igst: i64, cgst: i64, sgst: i64) -> Heads {
        Heads {
            igst,
            cgst,
            sgst,
            cess: 0,
        }
    }

    fn check_balanced(l: Heads, c: Heads, p: &OffsetProposal) {
        for (pay, due) in [
            (p.igst, l.igst),
            (p.cgst, l.cgst),
            (p.sgst, l.sgst),
            (p.cess, l.cess),
        ] {
            assert_eq!(pay.by_credit.total() + pay.cash, due.max(0));
        }
        let used = p
            .igst
            .by_credit
            .plus(p.cgst.by_credit)
            .plus(p.sgst.by_credit)
            .plus(p.cess.by_credit);
        assert_eq!(used.plus(p.carry_forward), c.floor());
        // CGST never pays SGST and the reverse.
        assert_eq!((p.cgst.by_credit.sgst, p.sgst.by_credit.cgst), (0, 0));
    }

    #[test]
    fn igst_credit_goes_to_igst_first() {
        let (l, c) = (h(1000, 500, 500), h(1200, 0, 0));
        let p = propose(l, c, Heads::default());
        assert_eq!(p.igst.by_credit.igst, 1000);
        assert_eq!(p.cgst.by_credit.igst + p.sgst.by_credit.igst, 200);
        assert_eq!(p.cash_total, 800);
        check_balanced(l, c, &p);
    }

    #[test]
    fn leftover_igst_credit_covers_the_shortfall_not_the_covered_head() {
        // CGST is covered by its own credit; SGST is not. The 300 of spare
        // IGST credit should go to SGST, leaving no cash to pay.
        let (l, c) = (h(0, 300, 300), h(300, 300, 0));
        let p = propose(l, c, Heads::default());
        assert_eq!(p.sgst.by_credit.igst, 300);
        assert_eq!(p.cgst.by_credit.cgst, 300);
        assert_eq!(p.cash_total, 0);
        check_balanced(l, c, &p);
    }

    #[test]
    fn cgst_and_sgst_credit_pay_igst_only_after_igst_credit() {
        let (l, c) = (h(1000, 0, 0), h(200, 500, 500));
        let p = propose(l, c, Heads::default());
        assert_eq!(
            p.igst.by_credit,
            Heads {
                igst: 200,
                cgst: 500,
                sgst: 300,
                cess: 0
            }
        );
        assert_eq!(p.carry_forward.sgst, 200);
        assert_eq!(p.cash_total, 0);
        check_balanced(l, c, &p);
    }

    #[test]
    fn reverse_charge_is_cash_and_cess_stays_cess() {
        let l = Heads {
            igst: 0,
            cgst: 100,
            sgst: 100,
            cess: 50,
        };
        let c = Heads {
            igst: 0,
            cgst: 500,
            sgst: 500,
            cess: 10,
        };
        let rc = h(180, 0, 0);
        let p = propose(l, c, rc);
        assert_eq!(p.cess.cash, 40);
        assert_eq!(p.cash_total, 40 + 180, "credit never pays reverse charge");
        assert_eq!(
            p.carry_forward,
            Heads {
                igst: 0,
                cgst: 400,
                sgst: 400,
                cess: 0
            }
        );
        check_balanced(l, c, &p);
    }

    #[test]
    fn negative_inputs_count_as_zero() {
        let p = propose(h(-5, 0, 0), h(-10, 0, 0), Heads::default());
        assert_eq!(p.cash_total, 0);
        assert_eq!(p.carry_forward, Heads::default());
    }
}
