#include "box3d_spike_shim.h"

#include <box3d/box3d.h>
#include <box3d/math_functions.h>
#include <stdlib.h>

#if defined( _WIN32 )
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#elif defined( __unix__ ) || defined( __APPLE__ )
#include <unistd.h>
#endif

struct B3SpikeWorld
{
    b3WorldId worldId;
    b3BodyId cubeId;
};

static int hardware_thread_count(void)
{
#if defined( _WIN32 )
    SYSTEM_INFO info;
    GetSystemInfo( &info );
    return (int)info.dwNumberOfProcessors;
#elif defined( __unix__ ) || defined( __APPLE__ )
    long count = sysconf( _SC_NPROCESSORS_ONLN );
    return count > 0 ? (int)count : 1;
#else
    return 1;
#endif
}

static int default_worker_count(void)
{
    int cores = hardware_thread_count();
    return b3ClampInt( cores / 2, 1, 8 );
}

B3SpikeWorld* b3sp_create_world(void)
{
    B3SpikeWorld* spike = (B3SpikeWorld*)calloc(1, sizeof(B3SpikeWorld));
    if (spike == NULL)
    {
        return NULL;
    }

    b3WorldDef worldDef = b3DefaultWorldDef();
    worldDef.gravity = (b3Vec3){ 0.0f, -9.8f, 0.0f };
    worldDef.enableSleep = true;
    worldDef.enableContinuous = true;
    worldDef.workerCount = (uint32_t)default_worker_count();
    spike->worldId = b3CreateWorld(&worldDef);

    b3BodyDef groundBodyDef = b3DefaultBodyDef();
    groundBodyDef.position = (b3Vec3){ 0.0f, -0.5f, 0.0f };
    b3BodyId groundId = b3CreateBody(spike->worldId, &groundBodyDef);

    b3BoxHull groundBox = b3MakeBoxHull(6.0f, 0.5f, 0.5f);
    b3ShapeDef groundShapeDef = b3DefaultShapeDef();
    groundShapeDef.baseMaterial.friction = 0.55f;
    groundShapeDef.baseMaterial.restitution = 0.1f;
    b3CreateHullShape(groundId, &groundShapeDef, &groundBox.base);

    b3BodyDef cubeBodyDef = b3DefaultBodyDef();
    cubeBodyDef.type = b3_dynamicBody;
    cubeBodyDef.position = (b3Vec3){ -1.25f, 3.5f, 0.0f };
    cubeBodyDef.linearVelocity = (b3Vec3){ 2.0f, 0.0f, 0.0f };
    cubeBodyDef.angularVelocity = (b3Vec3){ 0.0f, 0.0f, 2.75f };
    cubeBodyDef.linearDamping = 0.015f;
    cubeBodyDef.angularDamping = 0.025f;
    cubeBodyDef.motionLocks.linearZ = true;
    spike->cubeId = b3CreateBody(spike->worldId, &cubeBodyDef);

    b3BoxHull cubeBox = b3MakeCubeHull(0.5f);
    b3ShapeDef cubeShapeDef = b3DefaultShapeDef();
    cubeShapeDef.density = 1.0f;
    cubeShapeDef.baseMaterial.friction = 0.45f;
    cubeShapeDef.baseMaterial.restitution = 0.35f;
    b3CreateHullShape(spike->cubeId, &cubeShapeDef, &cubeBox.base);

    return spike;
}

void b3sp_step(B3SpikeWorld* spike, float timeStep, int subStepCount)
{
    if (spike == NULL)
    {
        return;
    }

    b3World_Step(spike->worldId, timeStep, subStepCount);
}

B3SpikeSnapshot b3sp_get_cube_snapshot(B3SpikeWorld* spike)
{
    B3SpikeSnapshot snapshot = { 0 };
    if (spike == NULL)
    {
        return snapshot;
    }

    b3Vec3 position = b3Body_GetPosition(spike->cubeId);
    b3Quat rotation = b3Body_GetRotation(spike->cubeId);
    snapshot.positionX = position.x;
    snapshot.positionY = position.y;
    snapshot.positionZ = position.z;
    snapshot.rotationX = rotation.v.x;
    snapshot.rotationY = rotation.v.y;
    snapshot.rotationZ = rotation.v.z;
    snapshot.rotationW = rotation.s;
    return snapshot;
}

void b3sp_destroy_world(B3SpikeWorld* spike)
{
    if (spike == NULL)
    {
        return;
    }

    b3DestroyWorld(spike->worldId);
    free(spike);
}
