#include "box3d_backend.h"

#include <box3d/box3d.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>

typedef struct SopBox3dBody
{
    uint64_t id;
    b3BodyId bodyId;
    b3ShapeId shapeId;
    float width;
    float height;
    bool isDragging;
    bool wasAwake;
} SopBox3dBody;

struct SopBox3dWorld
{
    b3WorldId worldId;
    SopBox3dBody* bodies;
    int bodyCount;
    int bodyCapacity;
    float boundsWidth;
    float boundsHeight;
    float pixelsPerMeter;
    float gravityY;
};

static b3Vec3 to_world_center(const SopBox3dWorld* world, float x, float y, float width, float height)
{
    return (b3Vec3){
        (x + width * 0.5f) / world->pixelsPerMeter,
        (world->boundsHeight - (y + height * 0.5f)) / world->pixelsPerMeter,
        0.0f
    };
}

static void from_world_center(const SopBox3dWorld* world, b3Vec3 center, float width, float height, float* x, float* y)
{
    *x = center.x * world->pixelsPerMeter - width * 0.5f;
    *y = world->boundsHeight - center.y * world->pixelsPerMeter - height * 0.5f;
}

static b3Vec3 to_world_velocity(const SopBox3dWorld* world, float x, float y)
{
    return (b3Vec3){ x / world->pixelsPerMeter, -y / world->pixelsPerMeter, 0.0f };
}

static float to_world_velocity_x(const SopBox3dWorld* world, float x)
{
    return x / world->pixelsPerMeter;
}

static void from_world_velocity(const SopBox3dWorld* world, b3Vec3 velocity, float* x, float* y)
{
    *x = velocity.x * world->pixelsPerMeter;
    *y = -velocity.y * world->pixelsPerMeter;
}

static b3Vec3 to_world_gravity(const SopBox3dWorld* world, float gravityY)
{
    return (b3Vec3){ 0.0f, -gravityY / world->pixelsPerMeter, 0.0f };
}

static SopBox3dBody* find_body(SopBox3dWorld* world, uint64_t id)
{
    for (int i = 0; i < world->bodyCount; ++i)
    {
        if (world->bodies[i].id == id)
        {
            return &world->bodies[i];
        }
    }

    return NULL;
}

static const SopBox3dBody* find_body_const(const SopBox3dWorld* world, int index)
{
    if (index < 0 || index >= world->bodyCount)
    {
        return NULL;
    }

    return &world->bodies[index];
}

static void ensure_capacity(SopBox3dWorld* world)
{
    if (world->bodyCount < world->bodyCapacity)
    {
        return;
    }

    int nextCapacity = world->bodyCapacity == 0 ? 16 : world->bodyCapacity * 2;
    SopBox3dBody* nextBodies = (SopBox3dBody*)realloc(world->bodies, (size_t)nextCapacity * sizeof(SopBox3dBody));
    if (nextBodies == NULL)
    {
        abort();
    }

    world->bodies = nextBodies;
    world->bodyCapacity = nextCapacity;
}

static void create_bounds(SopBox3dWorld* world)
{
    float widthMeters = fmaxf(world->boundsWidth / world->pixelsPerMeter, 0.1f);
    float heightMeters = fmaxf(world->boundsHeight / world->pixelsPerMeter, 0.1f);
    float thickness = 0.5f;

    b3ShapeDef shapeDef = b3DefaultShapeDef();
    shapeDef.baseMaterial.friction = 0.35f;
    shapeDef.baseMaterial.restitution = 0.2f;

    b3BodyDef bodyDef = b3DefaultBodyDef();
    b3BoxHull floorBox = b3MakeBoxHull(widthMeters * 0.5f, thickness * 0.5f, 1.0f);
    bodyDef.position = (b3Vec3){ widthMeters * 0.5f, -thickness * 0.5f, 0.0f };
    b3BodyId floorId = b3CreateBody(world->worldId, &bodyDef);
    b3CreateHullShape(floorId, &shapeDef, &floorBox.base);

    b3BoxHull ceilingBox = b3MakeBoxHull(widthMeters * 0.5f, thickness * 0.5f, 1.0f);
    bodyDef.position = (b3Vec3){ widthMeters * 0.5f, heightMeters + thickness * 0.5f, 0.0f };
    b3BodyId ceilingId = b3CreateBody(world->worldId, &bodyDef);
    b3CreateHullShape(ceilingId, &shapeDef, &ceilingBox.base);

    b3BoxHull sideBox = b3MakeBoxHull(thickness * 0.5f, heightMeters * 0.5f, 1.0f);
    bodyDef.position = (b3Vec3){ -thickness * 0.5f, heightMeters * 0.5f, 0.0f };
    b3BodyId leftId = b3CreateBody(world->worldId, &bodyDef);
    b3CreateHullShape(leftId, &shapeDef, &sideBox.base);

    bodyDef.position = (b3Vec3){ widthMeters + thickness * 0.5f, heightMeters * 0.5f, 0.0f };
    b3BodyId rightId = b3CreateBody(world->worldId, &bodyDef);
    b3CreateHullShape(rightId, &shapeDef, &sideBox.base);
}

static void create_world(SopBox3dWorld* world)
{
    b3WorldDef worldDef = b3DefaultWorldDef();
    worldDef.gravity = to_world_gravity(world, world->gravityY);
    worldDef.enableSleep = true;
    worldDef.enableContinuous = true;
    world->worldId = b3CreateWorld(&worldDef);
    create_bounds(world);
}

SopBox3dWorld* sop_box3d_create(float gravityY, float boundsWidth, float boundsHeight, float pixelsPerMeter)
{
    SopBox3dWorld* world = (SopBox3dWorld*)calloc(1, sizeof(SopBox3dWorld));
    if (world == NULL)
    {
        return NULL;
    }

    world->boundsWidth = fmaxf(boundsWidth, 1.0f);
    world->boundsHeight = fmaxf(boundsHeight, 1.0f);
    world->pixelsPerMeter = fmaxf(pixelsPerMeter, 1.0f);
    world->gravityY = gravityY;
    create_world(world);
    return world;
}

void sop_box3d_destroy(SopBox3dWorld* world)
{
    if (world == NULL)
    {
        return;
    }

    b3DestroyWorld(world->worldId);
    free(world->bodies);
    free(world);
}

void sop_box3d_reset(SopBox3dWorld* world, float gravityY, float boundsWidth, float boundsHeight)
{
    if (world == NULL)
    {
        return;
    }

    b3DestroyWorld(world->worldId);
    world->bodyCount = 0;
    world->boundsWidth = fmaxf(boundsWidth, 1.0f);
    world->boundsHeight = fmaxf(boundsHeight, 1.0f);
    world->gravityY = gravityY;
    create_world(world);
}

void sop_box3d_set_gravity(SopBox3dWorld* world, float gravityY)
{
    if (world == NULL)
    {
        return;
    }

    world->gravityY = gravityY;
    b3World_SetGravity(world->worldId, to_world_gravity(world, gravityY));
}

void sop_box3d_add_body(SopBox3dWorld* world, const SopBox3dBodyDef* def)
{
    if (world == NULL || def == NULL || find_body(world, def->id) != NULL)
    {
        return;
    }

    b3BodyDef bodyDef = b3DefaultBodyDef();
    bodyDef.type = def->isDragging ? b3_kinematicBody : b3_dynamicBody;
    bodyDef.position = to_world_center(world, def->x, def->y, def->width, def->height);
    bodyDef.linearVelocity = to_world_velocity(world, def->velocityX, def->velocityY);
    bodyDef.linearDamping = fmaxf(0.0f, (1.0f - def->linearDamping) * 8.0f);
    bodyDef.angularDamping = 0.18f;
    bodyDef.gravityScale = def->gravityScale;
    bodyDef.sleepThreshold = 0.08f;
    bodyDef.motionLocks.linearZ = true;
    bodyDef.enableSleep = true;
    bodyDef.isAwake = true;

    b3BodyId bodyId = b3CreateBody(world->worldId, &bodyDef);

    b3ShapeDef shapeDef = b3DefaultShapeDef();
    shapeDef.density = fmaxf(def->mass, 0.01f);
    shapeDef.baseMaterial.friction = fmaxf(def->friction, 0.0f);
    shapeDef.baseMaterial.restitution = fmaxf(def->restitution, 0.0f);

    b3ShapeId shapeId = b3_nullShapeId;
    float scale = fmaxf(def->collisionScale, 0.05f);
    if (def->shape == 1)
    {
        b3Sphere sphere = { { 0.0f, 0.0f, 0.0f }, fmaxf(fminf(def->width, def->height) * 0.5f * scale / world->pixelsPerMeter, 0.02f) };
        shapeId = b3CreateSphereShape(bodyId, &shapeDef, &sphere);
    }
    else if (def->shape == 2)
    {
        float halfHeight = fmaxf(def->height * 0.5f * scale / world->pixelsPerMeter, 0.02f);
        float radius = fmaxf(fminf(def->width, def->height) * 0.34f * scale / world->pixelsPerMeter, 0.02f);
        b3Vec3 points[6] = {
            { 0.0f, -halfHeight, 0.0f },
            { 0.0f, halfHeight, 0.0f },
            { 0.0f, 0.0f, radius },
            { radius, 0.0f, 0.0f },
            { 0.0f, 0.0f, -radius },
            { -radius, 0.0f, 0.0f },
        };
        b3HullData* diamond = b3CreateHull(points, 6, 6);
        if (diamond != NULL)
        {
            shapeId = b3CreateHullShape(bodyId, &shapeDef, diamond);
            b3DestroyHull(diamond);
        }
    }
    else
    {
        float hx = fmaxf(def->width * 0.5f * scale / world->pixelsPerMeter, 0.02f);
        float hy = fmaxf(def->height * 0.5f * scale / world->pixelsPerMeter, 0.02f);
        float hz = fmaxf(fminf(def->width, def->height) * 0.5f * scale / world->pixelsPerMeter, 0.02f);
        b3BoxHull box = b3MakeBoxHull(hx, hy, hz);
        shapeId = b3CreateHullShape(bodyId, &shapeDef, &box.base);
    }

    ensure_capacity(world);
    world->bodies[world->bodyCount++] = (SopBox3dBody){
        def->id,
        bodyId,
        shapeId,
        def->width,
        def->height,
        def->isDragging,
        true,
    };
}

void sop_box3d_sync_body(SopBox3dWorld* world, const SopBox3dBodyDef* def)
{
    if (world == NULL || def == NULL)
    {
        return;
    }

    SopBox3dBody* body = find_body(world, def->id);
    if (body == NULL)
    {
        sop_box3d_add_body(world, def);
        return;
    }

    body->width = def->width;
    body->height = def->height;

    b3Body_SetGravityScale(body->bodyId, def->gravityScale);
    b3Body_SetLinearDamping(body->bodyId, fmaxf(0.0f, (1.0f - def->linearDamping) * 8.0f));
    if (b3Shape_IsValid(body->shapeId))
    {
        b3Shape_SetFriction(body->shapeId, fmaxf(def->friction, 0.0f));
        b3Shape_SetRestitution(body->shapeId, fmaxf(def->restitution, 0.0f));
    }

    if (def->isDragging)
    {
        b3Body_SetType(body->bodyId, b3_kinematicBody);
        b3Body_SetTransform(body->bodyId, to_world_center(world, def->x, def->y, def->width, def->height), b3Body_GetRotation(body->bodyId));
        b3Body_SetLinearVelocity(body->bodyId, to_world_velocity(world, def->velocityX, def->velocityY));
        b3Body_SetAngularVelocity(body->bodyId, b3Vec3_zero);
        b3Body_SetAwake(body->bodyId, true);
        body->isDragging = true;
        body->wasAwake = true;
        return;
    }

    if (body->isDragging)
    {
        b3Body_SetType(body->bodyId, b3_dynamicBody);
        b3Body_SetTransform(body->bodyId, to_world_center(world, def->x, def->y, def->width, def->height), b3Body_GetRotation(body->bodyId));
        b3Body_SetLinearVelocity(body->bodyId, to_world_velocity(world, def->velocityX, def->velocityY));
        b3Body_SetAwake(body->bodyId, true);
        body->isDragging = false;
        body->wasAwake = true;
    }

    if (def->motorEnabled)
    {
        b3Vec3 velocity = b3Body_GetLinearVelocity(body->bodyId);
        velocity.x = to_world_velocity_x(world, def->motorVelocityX);
        b3Body_SetLinearVelocity(body->bodyId, velocity);
        b3Body_SetAwake(body->bodyId, true);
    }
}

void sop_box3d_step(SopBox3dWorld* world, float timeStep, int subStepCount)
{
    if (world == NULL)
    {
        return;
    }

    b3World_Step(world->worldId, timeStep, subStepCount);
}

int sop_box3d_snapshot_count(const SopBox3dWorld* world)
{
    return world == NULL ? 0 : world->bodyCount;
}

bool sop_box3d_get_snapshot(const SopBox3dWorld* world, int index, SopBox3dSnapshot* snapshot)
{
    if (world == NULL || snapshot == NULL)
    {
        return false;
    }

    const SopBox3dBody* body = find_body_const(world, index);
    if (body == NULL)
    {
        return false;
    }

    b3Vec3 position = b3Body_GetPosition(body->bodyId);
    b3Vec3 velocity = b3Body_GetLinearVelocity(body->bodyId);
    b3Quat rotation = b3Body_GetRotation(body->bodyId);

    memset(snapshot, 0, sizeof(*snapshot));
    snapshot->id = body->id;
    from_world_center(world, position, body->width, body->height, &snapshot->x, &snapshot->y);
    from_world_velocity(world, velocity, &snapshot->velocityX, &snapshot->velocityY);
    snapshot->rotationX = rotation.v.x;
    snapshot->rotationY = rotation.v.y;
    snapshot->rotationZ = rotation.v.z;
    snapshot->rotationW = rotation.s;
    snapshot->isAwake = b3Body_IsAwake(body->bodyId);
    return true;
}

int sop_box3d_get_snapshots(SopBox3dWorld* world, SopBox3dSnapshot* snapshots, int capacity)
{
    if (world == NULL || snapshots == NULL || capacity <= 0)
    {
        return 0;
    }

    int written = 0;
    for (int index = 0; index < world->bodyCount && written < capacity; ++index)
    {
        SopBox3dBody* body = &world->bodies[index];
        bool isAwake = b3Body_IsAwake(body->bodyId);
        if (!isAwake && !body->wasAwake && !body->isDragging)
        {
            continue;
        }

        b3Vec3 position = b3Body_GetPosition(body->bodyId);
        b3Vec3 velocity = b3Body_GetLinearVelocity(body->bodyId);
        b3Quat rotation = b3Body_GetRotation(body->bodyId);

        SopBox3dSnapshot* snapshot = &snapshots[written++];
        memset(snapshot, 0, sizeof(*snapshot));
        snapshot->id = body->id;
        from_world_center(world, position, body->width, body->height, &snapshot->x, &snapshot->y);
        from_world_velocity(world, velocity, &snapshot->velocityX, &snapshot->velocityY);
        snapshot->rotationX = rotation.v.x;
        snapshot->rotationY = rotation.v.y;
        snapshot->rotationZ = rotation.v.z;
        snapshot->rotationW = rotation.s;
        snapshot->isAwake = isAwake;
        body->wasAwake = isAwake;
    }

    return written;
}
