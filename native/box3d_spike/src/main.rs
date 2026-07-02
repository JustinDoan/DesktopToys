use std::ptr::NonNull;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct B3SpikeSnapshot {
    position_x: f32,
    position_y: f32,
    position_z: f32,
    rotation_x: f32,
    rotation_y: f32,
    rotation_z: f32,
    rotation_w: f32,
}

#[repr(C)]
struct B3SpikeWorld {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn b3sp_create_world() -> *mut B3SpikeWorld;
    fn b3sp_step(spike: *mut B3SpikeWorld, time_step: f32, sub_step_count: i32);
    fn b3sp_get_cube_snapshot(spike: *mut B3SpikeWorld) -> B3SpikeSnapshot;
    fn b3sp_destroy_world(spike: *mut B3SpikeWorld);
}

struct SpikeWorld {
    raw: NonNull<B3SpikeWorld>,
}

impl SpikeWorld {
    fn new() -> Self {
        let raw = unsafe { b3sp_create_world() };
        let raw = NonNull::new(raw).expect("Box3D spike world allocation failed");
        Self { raw }
    }

    fn step(&mut self, time_step: f32, sub_step_count: i32) {
        unsafe { b3sp_step(self.raw.as_ptr(), time_step, sub_step_count) };
    }

    fn cube_snapshot(&self) -> B3SpikeSnapshot {
        unsafe { b3sp_get_cube_snapshot(self.raw.as_ptr()) }
    }
}

impl Drop for SpikeWorld {
    fn drop(&mut self) {
        unsafe { b3sp_destroy_world(self.raw.as_ptr()) };
    }
}

fn main() {
    let mut world = SpikeWorld::new();
    println!("Box3D desktop-toy spike");
    println!("frame, pos_x, pos_y, pos_z, quat_x, quat_y, quat_z, quat_w");

    for frame in 0..=120 {
        let snapshot = world.cube_snapshot();
        if frame % 12 == 0 || frame == 120 {
            println!(
                "{frame:03}, {px:>7.3}, {py:>7.3}, {pz:>7.3}, {rx:>7.3}, {ry:>7.3}, {rz:>7.3}, {rw:>7.3}",
                px = snapshot.position_x,
                py = snapshot.position_y,
                pz = snapshot.position_z,
                rx = snapshot.rotation_x,
                ry = snapshot.rotation_y,
                rz = snapshot.rotation_z,
                rw = snapshot.rotation_w,
            );
        }

        world.step(1.0 / 60.0, 4);
    }
}
