//! Turns relative effort (weights per task per day) into hours that fill each
//! day's free capacity exactly, in fixed steps (e.g. 0.5h).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Demand {
    pub task: String,
    pub date: NaiveDate,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Allocation {
    pub task: String,
    pub date: NaiveDate,
    pub hours: f64,
}

/// For every date with capacity, splits it across that date's demands in
/// proportion to their weights. Results are multiples of `step` that sum to
/// the capacity (rounded down to a step); leftovers go to the largest
/// remainders, ties broken by weight then input order.
pub fn allocate(
    capacity: &BTreeMap<NaiveDate, f64>,
    demands: &[Demand],
    step: f64,
) -> Vec<Allocation> {
    let mut out = Vec::new();
    for (&date, &cap) in capacity {
        let today: Vec<&Demand> = demands
            .iter()
            .filter(|d| d.date == date && d.weight > EPS)
            .collect();
        let total_steps = (cap / step + EPS).floor() as i64;
        if today.is_empty() || total_steps <= 0 {
            continue;
        }
        let weight_sum: f64 = today.iter().map(|d| d.weight).sum();

        let mut parts: Vec<(usize, i64, f64)> = today
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let exact = total_steps as f64 * d.weight / weight_sum;
                (i, exact.floor() as i64, exact - exact.floor())
            })
            .collect();
        let mut left = total_steps - parts.iter().map(|p| p.1).sum::<i64>();

        let mut order: Vec<usize> = (0..parts.len()).collect();
        order.sort_by(|&a, &b| {
            parts[b]
                .2
                .partial_cmp(&parts[a].2)
                .unwrap()
                .then(today[b].weight.partial_cmp(&today[a].weight).unwrap())
                .then(a.cmp(&b))
        });
        for &i in order.iter().cycle() {
            if left == 0 {
                break;
            }
            parts[i].1 += 1;
            left -= 1;
        }

        for (i, steps, _) in parts {
            if steps > 0 {
                out.push(Allocation {
                    task: today[i].task.clone(),
                    date,
                    hours: steps as f64 * step,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn demand(task: &str, day: u32, weight: f64) -> Demand {
        Demand {
            task: task.into(),
            date: d(day),
            weight,
        }
    }

    fn hours_on(allocs: &[Allocation], day: u32) -> Vec<(String, f64)> {
        allocs
            .iter()
            .filter(|a| a.date == d(day))
            .map(|a| (a.task.clone(), a.hours))
            .collect()
    }

    #[test]
    fn fills_each_day_exactly_in_half_hours() {
        let cap = BTreeMap::from([(d(26), 8.0), (d(27), 4.0)]);
        let demands = vec![
            demand("a", 26, 0.6),
            demand("b", 26, 0.4),
            demand("a", 27, 1.0),
            demand("b", 27, 1.0),
            demand("c", 27, 1.0),
        ];
        let out = allocate(&cap, &demands, 0.5);

        assert_eq!(
            hours_on(&out, 26),
            vec![("a".into(), 5.0), ("b".into(), 3.0)]
        );
        let tue = hours_on(&out, 27);
        assert_eq!(tue.iter().map(|x| x.1).sum::<f64>(), 4.0);
        assert!(tue.iter().all(|x| (x.1 * 2.0).fract() == 0.0));
    }

    #[test]
    fn skips_days_without_capacity_or_demands() {
        let cap = BTreeMap::from([(d(26), 0.0), (d(27), 8.0), (d(28), 8.0)]);
        let demands = vec![
            demand("a", 26, 1.0),
            demand("a", 27, 1.0),
            demand("b", 29, 1.0),
        ];
        let out = allocate(&cap, &demands, 0.5);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].date, out[0].hours), (d(27), 8.0));
    }

    #[test]
    fn tiny_weights_can_round_to_nothing() {
        let cap = BTreeMap::from([(d(26), 1.0)]);
        let demands = vec![demand("big", 26, 10.0), demand("tiny", 26, 0.1)];
        let out = allocate(&cap, &demands, 0.5);
        assert_eq!(hours_on(&out, 26), vec![("big".into(), 1.0)]);
    }

    #[test]
    fn odd_capacity_rounds_down_to_step() {
        let cap = BTreeMap::from([(d(26), 2.75)]);
        let out = allocate(&cap, &[demand("a", 26, 1.0)], 0.5);
        assert_eq!(out[0].hours, 2.5);
    }
}
