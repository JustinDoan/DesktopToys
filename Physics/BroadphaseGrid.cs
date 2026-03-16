using System;
using System.Collections.Generic;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Physics;

public readonly record struct CollisionPair(int LeftIndex, int RightIndex);

public sealed class BroadphaseGrid
{
    private const float DefaultCellSize = 120f;

    private readonly float _cellSize;
    private readonly Dictionary<long, List<int>> _cells = [];
    private readonly List<long> _activeCellKeys = [];
    private readonly HashSet<ulong> _pairKeys = [];
    private readonly List<CollisionPair> _pairs = [];

    public BroadphaseGrid(float cellSize = DefaultCellSize)
    {
        _cellSize = Math.Max(16f, cellSize);
    }

    public IReadOnlyList<CollisionPair> BuildPairs(IReadOnlyList<ObjectState> objects)
    {
        ResetFrameState();
        FillCells(objects);
        BuildUniquePairs(objects);
        return _pairs;
    }

    private void ResetFrameState()
    {
        for (var i = 0; i < _activeCellKeys.Count; i++)
        {
            _cells[_activeCellKeys[i]].Clear();
        }

        _activeCellKeys.Clear();
        _pairKeys.Clear();
        _pairs.Clear();
    }

    private void FillCells(IReadOnlyList<ObjectState> objects)
    {
        for (var objectIndex = 0; objectIndex < objects.Count; objectIndex++)
        {
            var body = objects[objectIndex].Body;
            var minCellX = ToCellIndex(body.Position.X);
            var maxCellX = ToCellIndex(body.Position.X + body.Width);
            var minCellY = ToCellIndex(body.Position.Y);
            var maxCellY = ToCellIndex(body.Position.Y + body.Height);

            for (var cellY = minCellY; cellY <= maxCellY; cellY++)
            {
                for (var cellX = minCellX; cellX <= maxCellX; cellX++)
                {
                    var key = PackCellKey(cellX, cellY);
                    if (!_cells.TryGetValue(key, out var list))
                    {
                        list = [];
                        _cells.Add(key, list);
                    }

                    if (list.Count == 0)
                    {
                        _activeCellKeys.Add(key);
                    }

                    list.Add(objectIndex);
                }
            }
        }
    }

    private void BuildUniquePairs(IReadOnlyList<ObjectState> objects)
    {
        for (var keyIndex = 0; keyIndex < _activeCellKeys.Count; keyIndex++)
        {
            var list = _cells[_activeCellKeys[keyIndex]];
            for (var i = 0; i < list.Count; i++)
            {
                for (var j = i + 1; j < list.Count; j++)
                {
                    var left = list[i];
                    var right = list[j];
                    if (left == right)
                    {
                        continue;
                    }

                    var leftBody = objects[left].Body;
                    var rightBody = objects[right].Body;
                    if (leftBody.IsSleeping && rightBody.IsSleeping)
                    {
                        continue;
                    }

                    var minIndex = Math.Min(left, right);
                    var maxIndex = Math.Max(left, right);
                    var pairKey = ((ulong)(uint)minIndex << 32) | (uint)maxIndex;
                    if (!_pairKeys.Add(pairKey))
                    {
                        continue;
                    }

                    _pairs.Add(new CollisionPair(minIndex, maxIndex));
                }
            }
        }
    }

    private int ToCellIndex(float value)
    {
        return (int)MathF.Floor(value / _cellSize);
    }

    private static long PackCellKey(int cellX, int cellY)
    {
        return ((long)cellX << 32) | (uint)cellY;
    }
}
