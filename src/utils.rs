// Distribute integer quantities of a total according to un-normalized parts,
// minimizing the error incurred by rounding
// https://en.wikipedia.org/wiki/Largest_remainder_method
pub fn lrm(total: u32, parts: &Vec<u32>) -> Vec<u32> {
    let norm = parts.iter().fold(0, |i, a| i + *a);
    if norm == 0 {
        return vec![0; parts.len()];
    }
    let parts: Vec<_> = parts
        .into_iter()
        .map(|part| *part as f32 / norm as f32)
        .collect();
    // compute the ideal gains (real number)
    let fgains: Vec<f32> = parts.iter().map(|part| total as f32 * part).collect();
    // attribute the rounded down gains to everyone
    let mut gains: Vec<u32> = fgains.iter().map(|fgain| fgain.floor() as u32).collect();
    // compute the remaining quantity to distribute (guaranteed to be less than gains.len())
    let total = total - gains.iter().fold(0, |init, gain| init + gain);
    // give +1 to the largest remainders to distribute the remaining quantity
    let mut fgains_idx = fgains.iter().enumerate().collect::<Vec<_>>();
    fgains_idx.sort_unstable_by(|(_i1, fgain1), (_i2, fgain2)| {
        (**fgain1 - fgain1.floor())
            .partial_cmp(&(**fgain2 - fgain2.floor()))
            .unwrap()
    });
    fgains_idx.reverse();
    for (i, _) in fgains_idx.into_iter().take(total as usize) {
        gains[i] += 1;
    }
    gains
}
