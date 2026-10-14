//! The synthetic corpus model: a seeded RNG, clustered unit vectors, and words.
//!
//! Nothing here is derived from real content. The vector model is shaped to
//! the similarity statistics measured on the prod index on 2026-10-14
//! (in-database aggregates only; no vector or text left the database):
//!
//! | measured on prod | value |
//! |---|---|
//! | random pair, raw–raw, median cosine | 0.40 |
//! | nearest neighbour, median cosine | 0.94 |
//! | 10th neighbour, median | 0.87 |
//! | 50th neighbour, median | 0.82 |
//!
//! Uniform random vectors would be the wrong workload: in 1024 dimensions they
//! are all nearly orthogonal, which is much harder for HNSW than real
//! embeddings and would make every number here pessimistic. So a vector is a
//! weighted sum of a shared mean (what makes random pairs 0.4 alike), its
//! topic, its cluster, its near-duplicate group, and noise.

/// The vector width every bench vector has.
pub const DIMS: usize = fs3_store::EMBEDDING_DIMENSIONS;

/// xoshiro256** seeded through SplitMix64: small, fast, and reproducible
/// from one `u64`, which is all a bench needs.
#[derive(Clone, Debug)]
pub struct Rng {
    state: [u64; 4],
    spare: Option<f64>,
}

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut mix = seed;
        let mut next = || {
            mix = mix.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = mix;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Rng {
            state: [next(), next(), next(), next()],
            spare: None,
        }
    }

    /// An independent stream for `key`, so one part of the corpus can be
    /// regenerated without replaying everything before it.
    #[must_use]
    pub fn derive(seed: u64, key: u64) -> Self {
        Rng::new(seed ^ key.wrapping_mul(0xD6E8_FEB8_6659_FD93))
    }

    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= t;
        *s3 = s3.rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// Uniform in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.unit() * n as f64) as usize % n.max(1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Standard normal (Box–Muller, keeping the spare).
    pub fn gaussian(&mut self) -> f64 {
        if let Some(spare) = self.spare.take() {
            return spare;
        }
        let u = self.unit().max(f64::MIN_POSITIVE);
        let v = self.unit();
        let radius = (-2.0 * u.ln()).sqrt();
        let angle = std::f64::consts::TAU * v;
        self.spare = Some(radius * angle.sin());
        radius * angle.cos()
    }

    /// Log-normal with the given median and log-space spread.
    pub fn lognormal(&mut self, median: f64, sigma: f64) -> f64 {
        median * (sigma * self.gaussian()).exp()
    }

    /// Geometric count `>= 1` with the given mean.
    pub fn geometric(&mut self, mean: f64) -> usize {
        let p = 1.0 / mean.max(1.0);
        let mut n = 1;
        while !self.chance(p) && n < 10_000 {
            n += 1;
        }
        n
    }
}

/// A random unit direction.
#[must_use]
pub fn direction(rng: &mut Rng) -> Vec<f32> {
    let mut v: Vec<f32> = (0..DIMS).map(|_| rng.gaussian() as f32).collect();
    normalize(&mut v);
    v
}

pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

/// Shares of a vector's variance, from coarse to fine. Noise takes the rest.
const MEAN_SHARE: f32 = 0.38;
const TOPIC_SHARE: f32 = 0.17;
const CLUSTER_SHARE: (f32, f32) = (0.18, 0.32);
const GROUP_SHARE: (f32, f32) = (0.06, 0.14);
/// A query shares the coarse structure with its cluster but none of a group:
/// a question is near what answers it, not a near-duplicate of it.
const QUERY_CLUSTER_SHARE: f32 = 0.15;

/// How many vectors a cluster holds on average, at every scale. More content
/// means more clusters, not denser ones.
pub const VECTORS_PER_CLUSTER: usize = 400;
pub const TOPICS: usize = 64;

/// The fixed coarse structure: one mean, the topics, and each cluster's
/// centre, topic and tightness.
pub struct Space {
    seed: u64,
    mean: Vec<f32>,
    topics: Vec<Vec<f32>>,
    clusters: Vec<Cluster>,
}

struct Cluster {
    topic: usize,
    centre: Vec<f32>,
    share: f32,
}

/// Where a vector sits: its cluster, and its near-duplicate group.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub cluster: usize,
    pub group: u64,
}

impl Space {
    #[must_use]
    pub fn new(seed: u64, clusters: usize) -> Self {
        let mut rng = Rng::derive(seed, 1);
        let mean = direction(&mut rng);
        let topics = (0..TOPICS).map(|_| direction(&mut rng)).collect();
        let clusters = (0..clusters.max(TOPICS))
            .map(|index| Cluster {
                topic: index % TOPICS,
                centre: direction(&mut rng),
                share: CLUSTER_SHARE.0 + (CLUSTER_SHARE.1 - CLUSTER_SHARE.0) * rng.unit() as f32,
            })
            .collect();
        Space {
            seed,
            mean,
            topics,
            clusters,
        }
    }

    #[must_use]
    pub fn clusters(&self) -> usize {
        self.clusters.len()
    }

    /// A cluster in `topic`, chosen with `rng`.
    pub fn cluster_in(&self, topic: usize, rng: &mut Rng) -> usize {
        let per_topic = self.clusters.len() / TOPICS;
        (rng.below(per_topic.max(1)) * TOPICS + topic % TOPICS) % self.clusters.len()
    }

    /// One corpus vector at `place`. `point` makes it distinct within its group.
    #[must_use]
    pub fn vector(&self, place: Place, point: u64) -> Vec<f32> {
        let cluster = &self.clusters[place.cluster];
        let mut group_rng = Rng::derive(self.seed, place.group.wrapping_mul(31).wrapping_add(7));
        let group_share = GROUP_SHARE.0 + (GROUP_SHARE.1 - GROUP_SHARE.0) * group_rng.unit() as f32;
        let group = direction(&mut group_rng);
        let mut noise_rng = Rng::derive(self.seed ^ 0xA5A5, point);
        let noise_share = (1.0 - MEAN_SHARE - TOPIC_SHARE - cluster.share - group_share).max(0.0);
        let parts = [
            (MEAN_SHARE.sqrt(), &self.mean),
            (TOPIC_SHARE.sqrt(), &self.topics[cluster.topic]),
            (cluster.share.sqrt(), &cluster.centre),
            (group_share.sqrt(), &group),
        ];
        self.compose(&parts, noise_share.sqrt(), &mut noise_rng)
    }

    /// A query vector for `key`: never stored, so searching it starts cold.
    #[must_use]
    pub fn query(&self, key: u64) -> Vec<f32> {
        let mut rng = Rng::derive(self.seed ^ 0x5151, key);
        let cluster = &self.clusters[rng.below(self.clusters.len())];
        let noise_share = 1.0 - MEAN_SHARE - TOPIC_SHARE - QUERY_CLUSTER_SHARE;
        let parts = [
            (MEAN_SHARE.sqrt(), &self.mean),
            (TOPIC_SHARE.sqrt(), &self.topics[cluster.topic]),
            (QUERY_CLUSTER_SHARE.sqrt(), &cluster.centre),
        ];
        self.compose(&parts, noise_share.sqrt(), &mut rng)
    }

    fn compose(&self, parts: &[(f32, &Vec<f32>)], noise: f32, rng: &mut Rng) -> Vec<f32> {
        let mut v: Vec<f32> = (0..DIMS)
            .map(|_| noise * rng.gaussian() as f32 / (DIMS as f32).sqrt())
            .collect();
        for (weight, part) in parts {
            v.iter_mut()
                .zip(part.iter())
                .for_each(|(x, p)| *x += weight * p);
        }
        normalize(&mut v);
        v
    }
}

/// Synthetic words with a Zipf frequency, plus a few per topic, so text has
/// common and rare terms the way prose and code do.
pub struct Words {
    words: Vec<String>,
    cumulative: Vec<f64>,
}

const SYLLABLES: [&str; 48] = [
    "ka", "ri", "to", "ne", "su", "mo", "la", "vi", "de", "po", "zu", "fe", "ga", "hi", "jo", "ku",
    "me", "na", "or", "pa", "qui", "ro", "sa", "te", "ul", "va", "wo", "xa", "yu", "ze", "bri",
    "cla", "dro", "fli", "gro", "ple", "sta", "tri", "ven", "mar", "lin", "tor", "sen", "dal",
    "ric", "mun", "pex", "zor",
];

impl Words {
    #[must_use]
    pub fn new(seed: u64, count: usize) -> Self {
        let mut rng = Rng::derive(seed, 2);
        let mut seen = std::collections::HashSet::new();
        let mut words = Vec::with_capacity(count);
        while words.len() < count {
            let syllables = 2 + rng.below(3);
            let word: String = (0..syllables)
                .map(|_| SYLLABLES[rng.below(SYLLABLES.len())])
                .collect();
            if seen.insert(word.clone()) {
                words.push(word);
            }
        }
        let mut total = 0.0;
        let cumulative = (1..=count)
            .map(|rank| {
                total += 1.0 / (rank as f64).powf(1.05);
                total
            })
            .collect::<Vec<_>>();
        let cumulative = cumulative.iter().map(|c| c / total).collect();
        Words { words, cumulative }
    }

    /// The word at frequency rank `rank` (0 is the most common).
    #[must_use]
    pub fn at(&self, rank: usize) -> &str {
        &self.words[rank.min(self.words.len() - 1)]
    }

    pub fn zipf(&self, rng: &mut Rng) -> &str {
        let u = rng.unit();
        let rank = self.cumulative.partition_point(|c| *c < u);
        self.at(rank)
    }

    /// One of `topic`'s own words: mid-frequency, so a topic has a vocabulary.
    pub fn topical(&self, topic: usize, rng: &mut Rng) -> &str {
        self.at(2_000 + topic * 150 + rng.below(150))
    }

    /// Roughly `chars` of prose-like text in `topic`.
    pub fn text(&self, topic: usize, chars: usize, rng: &mut Rng) -> String {
        let mut out = String::with_capacity(chars + 16);
        let mut sentence = 0;
        while out.len() < chars {
            let word = if rng.chance(0.25) {
                self.topical(topic, rng)
            } else {
                self.zipf(rng)
            };
            out.push_str(word);
            sentence += 1;
            if sentence > 6 && rng.chance(0.15) {
                out.push_str(if rng.chance(0.2) { ".\n" } else { ". " });
                sentence = 0;
            } else {
                out.push(' ');
            }
        }
        out
    }

    /// A snake_case identifier in `topic`.
    pub fn identifier(&self, topic: usize, rng: &mut Rng) -> String {
        let parts = 2 + rng.below(2);
        (0..parts)
            .map(|i| {
                if i == 0 {
                    self.topical(topic, rng).to_string()
                } else {
                    self.zipf(rng).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    #[test]
    fn the_rng_is_reproducible_and_streams_differ() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_ne!(Rng::derive(7, 1).next_u64(), Rng::derive(7, 2).next_u64());
    }

    #[test]
    fn vectors_are_unit_and_clustered_like_the_measured_index() {
        let space = Space::new(11, 256);
        let near = |g: u64, p: u64| {
            space.vector(
                Place {
                    cluster: 3,
                    group: g,
                },
                p,
            )
        };
        let a = near(1, 1);
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-4);
        let same_group = cosine(&a, &near(1, 2));
        let same_cluster = cosine(&a, &near(2, 3));
        let elsewhere = cosine(
            &a,
            &space.vector(
                Place {
                    cluster: 200,
                    group: 9,
                },
                4,
            ),
        );
        assert!(same_group > 0.8, "near duplicate: {same_group}");
        assert!(
            (0.65..0.95).contains(&same_cluster),
            "same cluster: {same_cluster}"
        );
        assert!(
            (0.25..0.55).contains(&elsewhere),
            "random pair: {elsewhere}"
        );
        assert!(same_group > same_cluster && same_cluster > elsewhere);
    }

    #[test]
    fn words_follow_zipf() {
        let words = Words::new(3, 5_000);
        let mut rng = Rng::new(1);
        let draws: Vec<&str> = (0..20_000).map(|_| words.zipf(&mut rng)).collect();
        let top = draws.iter().filter(|w| **w == words.at(0)).count();
        let rare = draws.iter().filter(|w| **w == words.at(4_000)).count();
        assert!(top > 1_000 && rare < 10, "top {top}, rare {rare}");
    }
}
