use serde::{Deserialize, Serialize};
use serverbench_core::{statistics, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedianInterval {
    pub requested_coverage: f64,
    pub coverage_bound: Option<f64>,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    pub reason: Option<String>,
}
fn validate(values: &[f64]) -> Result<Vec<f64>> {
    if values.is_empty() || values.len() > 100 || values.iter().any(|x| !x.is_finite() || *x < 0.) {
        return Err(Error::new(
            4,
            "interval requires 1–100 finite nonnegative run values",
        ));
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Ok(sorted)
}
/// Integer binomial tails fit exactly in u128 for n <= 100; no factorial overflow.
pub fn median_interval(values: &[f64], coverage: f64) -> Result<MedianInterval> {
    let sorted = validate(values)?;
    if !coverage.is_finite() || !(0. ..1.).contains(&coverage) || coverage == 0. {
        return Err(Error::new(2, "coverage must be between zero and one"));
    }
    let n = sorted.len();
    let total = 1u128 << n;
    let mut tail = 0u128;
    let mut term = 1u128;
    let mut selected = None;
    for k in 1..=n.div_ceil(2) {
        tail += term;
        let bound = (total - 2 * tail) as f64 / total as f64;
        if bound >= coverage {
            selected = Some((k, bound));
        } else {
            break;
        }
        term = term * (n - k + 1) as u128 / k as u128;
    }
    Ok(match selected {
        Some((k, bound)) => MedianInterval {
            requested_coverage: coverage,
            coverage_bound: Some(bound),
            lower: Some(sorted[k - 1]),
            upper: Some(sorted[n - k]),
            reason: None,
        },
        None => MedianInterval {
            requested_coverage: coverage,
            coverage_bound: None,
            lower: None,
            upper: None,
            reason: Some("insufficient_repeats".into()),
        },
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatioInterval {
    pub point_ratio: Option<f64>,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    pub upper_unbounded: bool,
    pub joint_coverage_bound: Option<f64>,
    pub reason: Option<String>,
}
pub fn ratio_interval(a: &[f64], b: &[f64]) -> Result<RatioInterval> {
    validate(a)?;
    validate(b)?;
    let ma = statistics(a).median;
    let mb = statistics(b).median;
    let mut result = RatioInterval {
        point_ratio: (ma > 0.).then_some(mb / ma),
        lower: None,
        upper: None,
        upper_unbounded: false,
        joint_coverage_bound: None,
        reason: None,
    };
    if result.point_ratio.is_some_and(|v| !v.is_finite()) {
        return Err(Error::new(4, "ratio overflow"));
    }
    if ma == 0. {
        result.reason = Some("undefined_baseline".into());
        return Ok(result);
    }
    if a.len() < 10 || b.len() < 10 {
        result.reason = Some("insufficient_repeats".into());
        return Ok(result);
    }
    let ia = median_interval(a, 0.975)?;
    let ib = median_interval(b, 0.975)?;
    let (Some(la), Some(ua), Some(lb), Some(ub)) = (ia.lower, ia.upper, ib.lower, ib.upper) else {
        result.reason = Some("insufficient_repeats".into());
        return Ok(result);
    };
    result.lower = Some(lb / ua);
    result.joint_coverage_bound =
        Some(ia.coverage_bound.unwrap() + ib.coverage_bound.unwrap() - 1.);
    if la == 0. {
        result.upper_unbounded = true;
        result.reason = Some("unbounded_baseline".into());
    } else {
        let upper = ub / la;
        if !upper.is_finite() {
            return Err(Error::new(4, "interval ratio overflow"));
        }
        result.upper = Some(upper);
    }
    Ok(result)
}
