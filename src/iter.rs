// Copyright 2021 Travis Veazey
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// https://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// https://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::Rand;

use super::{Float, Poisson};
use kiddo::{KdTree, SquaredEuclidean};
use rand::prelude::*;
use rand_distr::StandardNormal;
use std::iter::FusedIterator;

#[cfg(test)]
mod tests;

/// A Point is simply an array of Float values
pub type Point<const N: usize> = [Float; N];

/// An iterator over the points in the Poisson disk distribution
pub struct Iter<const N: usize, U, R = Rand>
where
    U: Default + Clone,
    R: Rng + SeedableRng,
{
    /// The distribution from which this iterator was built
    distribution: Poisson<N, U, R>,
    /// The RNG
    rng: R,
    /// All previously-selected samples, to ensure new samples maintain minimum radius
    sampled: KdTree<Float, N>,
    /// A list of valid points that we have not yet visited
    active: Vec<Point<N>>,
}

impl<const N: usize, U, R> Iter<N, U, R>
where
    U: Default + Clone,
    R: Rng + SeedableRng,
{
    /// Create an iterator over the specified distribution
    pub(crate) fn new(distribution: Poisson<N, U, R>) -> Self {
        // If we were not given a seed, generate one non-deterministically
        let mut rng = match distribution.seed {
            None => R::from_entropy(),
            Some(seed) => R::seed_from_u64(seed),
        };

        // We have to generate an initial point, just to ensure we've got *something* in the active list
        let mut first_point = [0.0; N];
        for i in first_point.iter_mut() {
            // Start somewhere near the middle, but still randomly distributed
            *i = (0.5 - rng.gen::<Float>()) * distribution.radius;
        }

        Iter {
            distribution,
            rng,
            sampled: KdTree::new(),
            // Add our initial point to `active`, to give us somewhere to start, but don't add it to
            // `sampled` since this initial point never gets returned, creating a void in the output.
            // See #36
            active: vec![first_point],
        }
    }

    /// Add a point to our pattern
    fn add_point(&mut self, point: Point<N>) {
        // Add it to the active list
        self.active.push(point);

        // Now stash this point in our samples
        self.sampled.add(&point, 0);
    }

    /// Generate a random point between `radius` and `2 * radius` away from the given point
    fn generate_random_point(&mut self, around: Point<N>) -> Point<N> {
        // Pick a random distance away from our point
        let dist = self.distribution.radius * (1.0 + self.rng.gen::<Float>());

        // Generate a randomly distributed vector
        let mut vector: [Float; N] = [0.0; N];
        for i in vector.iter_mut() {
            *i = self.rng.sample(StandardNormal);
        }
        // Now find this new vector's magnitude
        let mag = vector.iter().map(|&x| x.powi(2)).sum::<Float>().sqrt();

        // Dividing each of the vector's components by `mag` will produce a unit vector; then by
        // multiplying each component by `dist`, we'll have a vector pointing `dist` away from the
        // origin. If we then add each of those components to our point, we'll have effectively
        // translated our point by `dist` in a randomly chosen direction.
        // Conveniently, we can do all of this in just one step!
        let mut point = [0.0; N];
        let translate = dist / mag; // compute this just once!
        for i in 0..N {
            point[i] = around[i] + vector[i] * translate;
        }

        point
    }

    /// Returns true if the point is within the bounds of our space.
    ///
    /// This is true if 0 ≤ point[i] < dimensions[i]
    fn in_space(&self, point: Point<N>) -> bool {
        (self.distribution.validate)(point, &self.distribution.validate_user_data)
    }

    /// Returns true if there is at least one other sample point within `radius` of this point
    fn in_neighborhood(&self, point: Point<N>) -> bool {
        !self
            .sampled
            .within::<SquaredEuclidean>(&point, self.distribution.radius.powi(2))
            .is_empty()
    }

    pub(crate) fn to_empty(mut self) -> Self {
        while self.next().is_some() {}
        self
    } 

    pub(crate) fn to_sampled(self) -> KdTree<Float, N> {
        self.sampled
    }
}

impl<const N: usize, U, R> Iterator for Iter<N, U, R>
where
    U: Default + Clone,
    R: Rng + SeedableRng,
{
    type Item = Point<N>;

    fn next(&mut self) -> Option<Point<N>> {
        while !self.active.is_empty() {
            dbg!(&self.active);

            let i = self.rng.gen_range(0..self.active.len());

            for _ in 0..self.distribution.num_samples {
                // Generate up to `num_samples` random points between radius and 2*radius from the current point
                let point = self.generate_random_point(self.active[i]);

                // Ensure we've picked a point inside the bounds of our rectangle, and more than `radius`
                // distance from any other sampled point
                if self.in_space(point) && !self.in_neighborhood(point) {
                    // We've got a good one!
                    self.add_point(point);

                    return Some(point);
                }
            }

            self.active.swap_remove(i);
        }

        None
    }
}

impl<const N: usize, U: Default + Clone> FusedIterator for Iter<N, U> {}
