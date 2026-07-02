#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct B3SpikeWorld B3SpikeWorld;

typedef struct B3SpikeSnapshot
{
    float positionX;
    float positionY;
    float positionZ;
    float rotationX;
    float rotationY;
    float rotationZ;
    float rotationW;
} B3SpikeSnapshot;

B3SpikeWorld* b3sp_create_world(void);
void b3sp_step(B3SpikeWorld* spike, float timeStep, int subStepCount);
B3SpikeSnapshot b3sp_get_cube_snapshot(B3SpikeWorld* spike);
void b3sp_destroy_world(B3SpikeWorld* spike);

#ifdef __cplusplus
}
#endif
