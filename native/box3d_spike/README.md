# Box3D Spike

This crate is an isolated probe for using Box3D from the native Rust path.
It does not change the app yet.

## Run

```powershell
cargo run -p box3d_spike
```

The spike builds Box3D v0.1.0 from `native/extern/box3d` through Cargo's C compiler support, then runs a tiny scene:

- static ground slab
- one dynamic cube
- gravity
- real Box3D position and quaternion output
- `linearZ` locked so the cube behaves like a screen-plane object

## What This Proves

- Box3D can be compiled into the Rust workspace without CMake.
- A small C shim is enough to avoid hand-writing a large Rust FFI surface.
- Motion locks can keep bodies on the overlay plane while still allowing 3D rotation.
- The render side should consume quaternions or matrices rather than the old Euler-only `RotationX/Y/Z` path.

## Integration Notes

The likely app-facing shape is a physics backend with methods like:

- create world with overlay bounds
- spawn box/sphere/convex proxy
- begin drag/end drag
- fixed-step simulation
- read body transform snapshots

Imported mesh objects should probably keep simple collision proxies at first. Box3D triangle mesh shapes are mainly useful for static colliders, not dynamic desktop toys.
