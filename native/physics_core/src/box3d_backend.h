#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SopBox3dWorld SopBox3dWorld;

typedef struct SopBox3dBodyDef
{
    uint64_t id;
    float x;
    float y;
    float width;
    float height;
    float velocityX;
    float velocityY;
    float mass;
    float restitution;
    float friction;
    float linearDamping;
    float gravityScale;
    bool motorEnabled;
    float motorVelocityX;
    float collisionScale;
    int shape;
    bool isDragging;
} SopBox3dBodyDef;

typedef struct SopBox3dSnapshot
{
    uint64_t id;
    float x;
    float y;
    float velocityX;
    float velocityY;
    float rotationX;
    float rotationY;
    float rotationZ;
    float rotationW;
    bool isAwake;
} SopBox3dSnapshot;

SopBox3dWorld* sop_box3d_create(float gravityY, float boundsWidth, float boundsHeight, float pixelsPerMeter);
void sop_box3d_destroy(SopBox3dWorld* world);
void sop_box3d_reset(SopBox3dWorld* world, float gravityY, float boundsWidth, float boundsHeight);
void sop_box3d_set_gravity(SopBox3dWorld* world, float gravityY);
void sop_box3d_add_body(SopBox3dWorld* world, const SopBox3dBodyDef* def);
void sop_box3d_sync_body(SopBox3dWorld* world, const SopBox3dBodyDef* def);
void sop_box3d_step(SopBox3dWorld* world, float timeStep, int subStepCount);
int sop_box3d_snapshot_count(const SopBox3dWorld* world);
bool sop_box3d_get_snapshot(const SopBox3dWorld* world, int index, SopBox3dSnapshot* snapshot);
int sop_box3d_get_snapshots(SopBox3dWorld* world, SopBox3dSnapshot* snapshots, int capacity);

#ifdef __cplusplus
}
#endif
