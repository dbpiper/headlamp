pub(super) fn compute_column_widths(
    total_width: usize,
    mins: &[usize],
    maxs: &[usize],
    column_count: usize,
) -> Vec<usize> {
    let borders = column_count + 1;
    let budget = total_width.saturating_sub(borders).max(1);

    let min_sum = mins.iter().copied().sum::<usize>();
    let max_sum = maxs.iter().copied().sum::<usize>();

    let mut widths = mins.to_vec();
    if min_sum > budget {
        let factor = (budget as f64) / (min_sum as f64);
        widths = mins
            .iter()
            .map(|m| ((*m as f64) * factor).floor().max(1.0) as usize)
            .collect::<Vec<_>>();
        let mut leftover = budget.saturating_sub(widths.iter().copied().sum::<usize>());
        for w in widths.iter_mut() {
            if leftover == 0 {
                break;
            }
            *w += 1;
            leftover -= 1;
        }
        return widths;
    }

    let mut remaining = budget.min(max_sum).saturating_sub(min_sum);
    for i in 0..widths.len() {
        if remaining == 0 {
            break;
        }
        let grow = remaining.min(maxs[i].saturating_sub(widths[i]));
        widths[i] += grow;
        remaining -= grow;
    }

    widths
}
