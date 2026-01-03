use bevy_render::render_resource::{
    encase::internal::WriteInto, BufferUsages, BufferVec, ShaderType,
};
use core::hint::black_box;
use criterion::{criterion_group, Criterion};
use glam::{Vec3, Vec4};
use rand::{
    distr::{Distribution, StandardUniform},
    prelude::*,
};

#[derive(ShaderType, Copy, Clone)]
struct BigItem {
    position: Vec3,
    // GPU side padding: 1 byte
    #[align(16)]
    weight: f32,
    // GPU side padding: 12 bytes
    // #[align(16)]
    gradient: [Vec4; 4],
    lut: [[f32; 4]; 16],
}

#[derive(ShaderType, Copy, Clone)]
struct Item {
    x: f32,
    // GPU side padding: 12 bytes
    y: Vec3,
    // GPU side padding: 1 byte
}

// This trait simplifies correctness tests and helps label benchmarks
trait ShaderData: ShaderType + WriteInto + Copy {
    const LABEL: &'static str;

    fn mightbe_uninit(_i: usize) -> bool {
        false
    }
}

impl ShaderData for f32 {
    const LABEL: &'static str = "f32";
}

impl ShaderData for Vec4 {
    const LABEL: &'static str = "vec4";
}

impl<const N: usize> ShaderData for [Vec4; N] {
    const LABEL: &'static str = "vec4-arr";
}

impl ShaderData for Item {
    const LABEL: &'static str = "item";
    fn mightbe_uninit(i: usize) -> bool {
        let offset = i % u64::from(Self::min_size()) as usize;
        matches!(offset, 4..16 | 28..32)
    }
}

impl ShaderData for BigItem {
    const LABEL: &'static str = "big-item";
    fn mightbe_uninit(i: usize) -> bool {
        let offset = i % u64::from(Self::min_size()) as usize;
        matches!(offset, 4..16 | 20..32)
    }
}

impl Distribution<BigItem> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BigItem {
        BigItem {
            position: rng.random(),
            weight: rng.random(),
            gradient: rng.random(),
            lut: rng.random(),
        }
    }
}

impl Distribution<Item> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Item {
        Item {
            x: rng.random(),
            y: rng.random(),
        }
    }
}

const N_ITEMS: usize = 10_000;

fn buffer_vec_push_compare(c: &mut Criterion) {
    let mut rng = rand::rng();

    let data: Vec<f32> = (0..3 * N_ITEMS).map(|_| rng.random()).collect();
    buffer_vec_push_data(c, &data);

    let data: Vec<Vec4> = (0..3 * N_ITEMS).map(|_| rng.random()).collect();
    buffer_vec_push_data(c, &data);

    let data: Vec<Item> = (0..3 * N_ITEMS).map(|_| rng.random()).collect();
    buffer_vec_push_data(c, &data);

    let data: Vec<BigItem> = (0..3 * N_ITEMS).map(|_| rng.random()).collect();
    buffer_vec_push_data(c, &data);
}

fn buffer_vec_push_data<T: ShaderData>(c: &mut Criterion, data3: &[T]) {
    let data = &data3[..data3.len() / 3];
    c.bench_function(&format!("push-{}", T::LABEL), |b| {
        b.iter(|| {
            let mut buffer = BufferVec::<T>::new(BufferUsages::empty());
            for v in data {
                black_box(buffer.push(*v));
            }
        });
    });

    let data = &data3[2 * data3.len() / 3..];
    c.bench_function(&format!("push-fast-{}", T::LABEL), |b| {
        b.iter(|| {
            let mut buffer = BufferVec::<T>::new(BufferUsages::empty());
            for v in data {
                black_box(buffer.push_fast(*v));
            }
        });
    });

    let data = &data3[data3.len() / 3..2 * data3.len() / 3];
    c.bench_function(&format!("push-fast-alt-{}", T::LABEL), |b| {
        b.iter(|| {
            let mut buffer = BufferVec::<T>::new(BufferUsages::empty());
            for v in data {
                black_box(buffer.push_fast_alt(*v));
            }
        });
    });
}

criterion_group!(benches, buffer_vec_push_compare);

// /!\ This is unsound, because we read into uninitialized data.
// get_data() produces a reference to uninitialized data, which by itself
// is unsound.
fn compare_with_uninit<T: ShaderData>(left: &BufferVec<T>, right: &BufferVec<T>) {
    assert_eq!(left.len(), right.len());
    assert_eq!(left.get_data().len(), right.get_data().len());
    let left = left.get_data();
    let right = right.get_data();
    for (i, (a, b)) in left.iter().zip(right).enumerate() {
        if a != b {
            let offset = i % u64::from(T::min_size()) as usize;
            println!(
                "Bytes {} / {} differ ({} != {}) [#{}, {}]",
                i,
                N_ITEMS * u64::from(T::min_size()) as usize - 1,
                *a,
                *b,
                offset,
                if T::mightbe_uninit(i) {
                    "expected"
                } else {
                    "oops!"
                },
            );
            assert!(T::mightbe_uninit(i));
        }
    }
}

#[inline(never)]
fn check_correctness<T: ShaderData>(data: &[T]) {
    println!("Checking BufferVec<{}>", T::LABEL);
    println!(
        "\t{}::min_size() = {}",
        T::LABEL,
        u64::from(T::min_size()) as usize
    );
    println!("\tsizeof::<{}>() = {}", T::LABEL, size_of::<T>());
    println!("\talignof::<{}>() = {}", T::LABEL, align_of::<T>());

    let mut buffer_ref = BufferVec::<T>::new(BufferUsages::empty());
    let mut buffer_no_zero = BufferVec::<T>::new(BufferUsages::empty());
    let mut buffer_alt_no_zero = BufferVec::<T>::new(BufferUsages::empty());
    for v in data {
        buffer_ref.push(*v);
        buffer_no_zero.push_fast(*v);
        buffer_alt_no_zero.push_fast_alt(*v);
    }

    // The following calls are unsound (see comment)
    compare_with_uninit(&buffer_ref, &buffer_no_zero);
    compare_with_uninit(&buffer_ref, &buffer_alt_no_zero);
}

fn main() {
    let mut rng = rand::rng();

    let rands: Vec<f32> = (0..N_ITEMS).map(|_| rng.random()).collect();
    check_correctness(&rands);

    let rands: Vec<[Vec4; 10]> = (0..N_ITEMS).map(|_| rng.random()).collect();
    check_correctness(&rands);

    let rands: Vec<Item> = (0..N_ITEMS).map(|_| rng.random()).collect();
    check_correctness(&rands);

    let rands: Vec<BigItem> = (0..N_ITEMS).map(|_| rng.random()).collect();
    check_correctness(&rands);
}
