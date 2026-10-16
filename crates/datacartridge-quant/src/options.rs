//! Option pricing: Black-Scholes-Merton with greeks, implied volatility, and a
//! Cox-Ross-Rubinstein lattice for American exercise.

use datacartridge_core::error::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::{bisect, finite};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Right {
    Call,
    Put,
}

/// Inputs of the Black-Scholes-Merton model.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct OptionInput {
    pub right: Right,
    pub spot: f64,
    pub strike: f64,
    /// Continuously compounded risk-free rate.
    pub rate: f64,
    /// Continuous dividend yield.
    pub dividend_yield: f64,
    /// Annualized volatility.
    pub volatility: f64,
    /// Time to expiry in years.
    pub time: f64,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Greeks {
    pub price: f64,
    pub delta: f64,
    pub gamma: f64,
    /// Price change per 1.00 (100 vol points) change in volatility.
    pub vega: f64,
    /// Price change per year of time decay.
    pub theta: f64,
    /// Price change per 1.00 change in the rate.
    pub rho: f64,
}

/// Standard normal density.
pub fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// Standard normal distribution function, accurate to double precision (West, 2005).
pub fn norm_cdf(x: f64) -> f64 {
    let a = x.abs();
    let tail = if a > 37.0 {
        0.0
    } else {
        let e = (-a * a / 2.0).exp();
        if a < 7.071_067_811_865_47 {
            let mut num = 3.526_249_659_989_11e-2 * a + 0.700_383_064_443_688;
            num = num * a + 6.373_962_203_531_65;
            num = num * a + 33.912_866_078_383;
            num = num * a + 112.079_291_497_871;
            num = num * a + 221.213_596_169_931;
            num = num * a + 220.206_867_912_376;
            let mut den = 8.838_834_764_831_84e-2 * a + 1.755_667_163_182_64;
            den = den * a + 16.064_177_579_207;
            den = den * a + 86.780_732_202_946_1;
            den = den * a + 296.564_248_779_674;
            den = den * a + 637.333_633_378_831;
            den = den * a + 793.826_512_519_948;
            den = den * a + 440.413_735_824_752;
            e * num / den
        } else {
            let mut b = a + 0.65;
            b = a + 4.0 / b;
            b = a + 3.0 / b;
            b = a + 2.0 / b;
            b = a + 1.0 / b;
            e / b / 2.506_628_274_631
        }
    };
    if x > 0.0 { 1.0 - tail } else { tail }
}

impl OptionInput {
    fn validate(&self) -> Result<()> {
        finite("option", &[self.spot, self.strike, self.rate, self.dividend_yield, self.volatility, self.time])?;
        if self.spot <= 0.0 || self.strike <= 0.0 {
            return Err(Error::invalid("option", "spot and strike must be positive".to_string()));
        }
        if self.volatility <= 0.0 || self.time <= 0.0 {
            return Err(Error::invalid("option", "volatility and time to expiry must be positive".to_string()));
        }
        Ok(())
    }

    fn d1_d2(&self) -> (f64, f64) {
        let vt = self.volatility * self.time.sqrt();
        let d1 = ((self.spot / self.strike).ln() + (self.rate - self.dividend_yield + 0.5 * self.volatility.powi(2)) * self.time) / vt;
        (d1, d1 - vt)
    }

    /// European price and sensitivities.
    pub fn greeks(&self) -> Result<Greeks> {
        self.validate()?;
        let (d1, d2) = self.d1_d2();
        let (s, k, r, q, v, t) = (self.spot, self.strike, self.rate, self.dividend_yield, self.volatility, self.time);
        let df_q = (-q * t).exp();
        let df_r = (-r * t).exp();
        let gamma = df_q * norm_pdf(d1) / (s * v * t.sqrt());
        let vega = s * df_q * norm_pdf(d1) * t.sqrt();
        let decay = -s * df_q * norm_pdf(d1) * v / (2.0 * t.sqrt());
        Ok(match self.right {
            Right::Call => Greeks {
                price: s * df_q * norm_cdf(d1) - k * df_r * norm_cdf(d2),
                delta: df_q * norm_cdf(d1),
                gamma,
                vega,
                theta: decay - r * k * df_r * norm_cdf(d2) + q * s * df_q * norm_cdf(d1),
                rho: k * t * df_r * norm_cdf(d2),
            },
            Right::Put => Greeks {
                price: k * df_r * norm_cdf(-d2) - s * df_q * norm_cdf(-d1),
                delta: -df_q * norm_cdf(-d1),
                gamma,
                vega,
                theta: decay + r * k * df_r * norm_cdf(-d2) - q * s * df_q * norm_cdf(-d1),
                rho: -k * t * df_r * norm_cdf(-d2),
            },
        })
    }

    pub fn price(&self) -> Result<f64> {
        Ok(self.greeks()?.price)
    }

    /// The volatility at which the model reproduces `market_price`. Fails when the price
    /// violates the no-arbitrage bounds, where no volatility exists.
    pub fn implied_volatility(&self, market_price: f64) -> Result<f64> {
        let probe = OptionInput { volatility: 1.0, ..*self };
        probe.validate()?;
        if !market_price.is_finite() {
            return Err(Error::invalid("implied volatility", "price is not finite".to_string()));
        }
        let fwd_spot = self.spot * (-self.dividend_yield * self.time).exp();
        let pv_strike = self.strike * (-self.rate * self.time).exp();
        let (lower, upper) = match self.right {
            Right::Call => ((fwd_spot - pv_strike).max(0.0), fwd_spot),
            Right::Put => ((pv_strike - fwd_spot).max(0.0), pv_strike),
        };
        if market_price <= lower || market_price >= upper {
            return Err(Error::invalid(
                "implied volatility",
                format!("price {market_price} is outside the no-arbitrage bounds ({lower}, {upper})"),
            ));
        }
        let f = |v: f64| OptionInput { volatility: v, ..*self }.price().unwrap_or(f64::NAN) - market_price;
        let (lo, hi) = (1e-6, 20.0);
        if !(f(lo) < 0.0 && f(hi) > 0.0) {
            return Err(Error::invalid("implied volatility", "no volatility between 0.0001% and 2000% reproduces the price".to_string()));
        }
        Ok(bisect(lo, hi, f))
    }

    /// American-exercise price on a CRR binomial tree with `steps` steps.
    pub fn american_price(&self, steps: usize) -> Result<f64> {
        self.validate()?;
        if steps == 0 {
            return Err(Error::invalid("option", "the tree needs at least one step".to_string()));
        }
        let dt = self.time / steps as f64;
        let u = (self.volatility * dt.sqrt()).exp();
        let d = 1.0 / u;
        let growth = ((self.rate - self.dividend_yield) * dt).exp();
        let p = (growth - d) / (u - d);
        if !(0.0..=1.0).contains(&p) {
            return Err(Error::invalid("option", format!("{steps} steps are too few for these rates and volatility (risk-neutral probability {p})")));
        }
        let discount = (-self.rate * dt).exp();
        let payoff = |s: f64| match self.right {
            Right::Call => (s - self.strike).max(0.0),
            Right::Put => (self.strike - s).max(0.0),
        };
        let mut values: Vec<f64> = (0..=steps).map(|j| payoff(self.spot * u.powi(j as i32) * d.powi((steps - j) as i32))).collect();
        for step in (0..steps).rev() {
            for j in 0..=step {
                let hold = discount * (p * values[j + 1] + (1.0 - p) * values[j]);
                let exercise = payoff(self.spot * u.powi(j as i32) * d.powi((step - j) as i32));
                values[j] = hold.max(exercise);
            }
        }
        Ok(values[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    fn atm(right: Right) -> OptionInput {
        OptionInput { right, spot: 100.0, strike: 100.0, rate: 0.05, dividend_yield: 0.0, volatility: 0.2, time: 1.0 }
    }

    #[test]
    fn normal_distribution_reference_values() {
        assert!(close(norm_cdf(0.0), 0.5, 1e-15));
        assert!(close(norm_cdf(1.0), 0.841_344_746_068_542_9, 1e-14));
        assert!(close(norm_cdf(1.96), 0.975_002_104_851_779_5, 1e-14));
        assert!(close(norm_cdf(-3.0), 0.001_349_898_031_630_094_6, 1e-15));
        assert!(close(norm_cdf(8.0) + norm_cdf(-8.0), 1.0, 1e-15));
        assert!(norm_cdf(-40.0) == 0.0 && norm_cdf(40.0) == 1.0);
    }

    #[test]
    fn black_scholes_reference_values() {
        let c = atm(Right::Call).greeks().unwrap();
        assert!(close(c.price, 10.450_583_572_185_565, 1e-10));
        assert!(close(c.delta, 0.636_830_651_175_619_1, 1e-10));
        assert!(close(c.gamma, 0.018_762_017_345_846_895, 1e-10));
        assert!(close(c.vega, 37.524_034_691_693_79, 1e-9));
        assert!(close(c.theta, -6.414_027_546_438_197, 1e-9));
        assert!(close(c.rho, 53.232_481_545_376_345, 1e-9));
        let p = atm(Right::Put).greeks().unwrap();
        assert!(close(p.price, 5.573_526_022_256_971, 1e-10));
        assert!(close(p.delta, c.delta - 1.0, 1e-12));
    }

    #[test]
    fn put_call_parity_with_dividends() {
        let base = OptionInput { right: Right::Call, spot: 87.0, strike: 95.0, rate: 0.03, dividend_yield: 0.02, volatility: 0.35, time: 0.75 };
        let call = base.price().unwrap();
        let put = OptionInput { right: Right::Put, ..base }.price().unwrap();
        let parity = base.spot * (-0.02f64 * 0.75).exp() - base.strike * (-0.03f64 * 0.75).exp();
        assert!(close(call - put, parity, 1e-10));
    }

    #[test]
    fn greeks_match_finite_differences() {
        let base = OptionInput { right: Right::Put, spot: 105.0, strike: 100.0, rate: 0.04, dividend_yield: 0.01, volatility: 0.25, time: 0.5 };
        let g = base.greeks().unwrap();
        let price = |f: &dyn Fn(&mut OptionInput)| {
            let mut o = base;
            f(&mut o);
            o.price().unwrap()
        };
        let h = 1e-4;
        let delta = (price(&|o| o.spot += h) - price(&|o| o.spot -= h)) / (2.0 * h);
        let vega = (price(&|o| o.volatility += h) - price(&|o| o.volatility -= h)) / (2.0 * h);
        let rho = (price(&|o| o.rate += h) - price(&|o| o.rate -= h)) / (2.0 * h);
        let theta = -(price(&|o| o.time += h) - price(&|o| o.time -= h)) / (2.0 * h);
        assert!(close(g.delta, delta, 1e-6) && close(g.vega, vega, 1e-5) && close(g.rho, rho, 1e-5) && close(g.theta, theta, 1e-5));
    }

    #[test]
    fn implied_volatility_round_trip_and_bounds() {
        let o = OptionInput { right: Right::Call, spot: 100.0, strike: 110.0, rate: 0.02, dividend_yield: 0.0, volatility: 0.37, time: 0.4 };
        let price = o.price().unwrap();
        assert!(close(o.implied_volatility(price).unwrap(), 0.37, 1e-9));
        // Below intrinsic or above spot there is no volatility.
        let itm = OptionInput { strike: 50.0, ..o };
        assert!(itm.implied_volatility(49.0).is_err());
        assert!(o.implied_volatility(100.0).is_err());
        assert!(o.implied_volatility(0.0).is_err());
    }

    #[test]
    fn american_exercise() {
        // Without dividends an American call is worth its European price.
        let call = atm(Right::Call);
        assert!(close(call.american_price(2000).unwrap(), call.price().unwrap(), 5e-3));
        // An American put carries an early-exercise premium (reference value 6.0896).
        let put = atm(Right::Put);
        let american = put.american_price(2000).unwrap();
        assert!(close(american, 6.0896, 5e-3), "{american}");
        assert!(american > put.price().unwrap() + 0.4);
        // Deep in the money it is worth at least intrinsic value.
        let deep = OptionInput { spot: 50.0, ..put };
        assert!(deep.american_price(500).unwrap() >= 50.0);
        assert!(put.american_price(0).is_err());
    }

    #[test]
    fn invalid_inputs() {
        assert!(OptionInput { volatility: 0.0, ..atm(Right::Call) }.price().is_err());
        assert!(OptionInput { time: -1.0, ..atm(Right::Call) }.price().is_err());
        assert!(OptionInput { spot: f64::NAN, ..atm(Right::Call) }.price().is_err());
    }
}
