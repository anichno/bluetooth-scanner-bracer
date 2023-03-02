#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MovingAvg<const N: usize> {
    data: [i32; N],
    num_samples: usize,
}

impl<const N: usize> MovingAvg<N> {
    pub fn new() -> Self {
        Self {
            data: [0; N],
            num_samples: 0,
        }
    }

    fn is_filled(&self) -> bool {
        self.num_samples == N
    }

    /// Push a new value into the moving average
    pub fn push(&mut self, val: i32) {
        self.data.rotate_right(1);
        self.data[0] = val;
        self.num_samples = N.min(self.num_samples + 1);
    }

    pub fn peek_last(&self) -> i32 {
        self.data[0]
    }

    pub fn get_avg(&self) -> i32 {
        if self.is_filled() {
            self.data.iter().sum::<i32>() / N as i32
        } else {
            self.data.iter().take(self.num_samples).sum::<i32>() / self.num_samples as i32
        }
    }
}

pub fn num_linear_conversion(
    val: f32,
    in_min: f32,
    in_max: f32,
    out_min: f32,
    out_max: f32,
) -> f32 {
    let val = num::clamp(val, in_min, in_max);
    ((val - in_min) / (in_max - in_min)) * (out_max - out_min) + out_min
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moving_avg() {
        let mut avg = MovingAvg::<3>::new();
        avg.push(1);
        avg.push(2);
        avg.push(3);
        assert_eq!(avg.get_avg(), 2);
        avg.push(4);
        assert_eq!(avg.get_avg(), 3);
        avg.push(5);
        assert_eq!(avg.get_avg(), 4);
        avg.push(6);
        assert_eq!(avg.get_avg(), 5);
    }

    #[test]
    fn peek_last() {
        let mut avg = MovingAvg::<3>::new();
        avg.push(1);
        assert_eq!(avg.peek_last(), 1);
        avg.push(2);
        assert_eq!(avg.peek_last(), 2);
        avg.push(3);
        assert_eq!(avg.peek_last(), 3);
        avg.push(4);
        assert_eq!(avg.peek_last(), 4);
        avg.push(5);
        assert_eq!(avg.peek_last(), 5);
        avg.push(6);
        assert_eq!(avg.peek_last(), 6);
    }
}
