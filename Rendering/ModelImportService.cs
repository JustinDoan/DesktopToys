using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Windows.Media;
using System.Windows.Media.Media3D;
using Assimp;
using HelixToolkit.Wpf;

namespace ScreenOverlayPhysics.Rendering;

public static class ModelImportService
{
    private static readonly Dictionary<string, Model3DGroup> Cache = [];

    public static Model3DGroup LoadNormalized(string path, double targetSize, Color tint)
    {
        var fullPath = Path.GetFullPath(path);
        var cacheKey = string.Create(
            CultureInfo.InvariantCulture,
            $"{fullPath}|{targetSize:0.###}");

        if (Cache.TryGetValue(cacheKey, out var cached))
        {
            var cachedClone = cached.Clone();
            ApplyTint(cachedClone, tint);
            return cachedClone;
        }

        if (!File.Exists(fullPath))
        {
            throw new FileNotFoundException("Model file not found.", fullPath);
        }

        var imported = Import(fullPath);
        var normalized = Normalize(imported, targetSize);
        normalized.Freeze();
        Cache[cacheKey] = normalized;
        var clone = normalized.Clone();
        ApplyTint(clone, tint);
        return clone;
    }

    private static Model3DGroup Import(string path)
    {
        var extension = Path.GetExtension(path).ToLowerInvariant();
        return extension switch
        {
            ".obj" => new ObjReader().Read(path) ?? throw new InvalidOperationException("OBJ import returned no model."),
            ".stl" => new StLReader().Read(path) ?? throw new InvalidOperationException("STL import returned no model."),
            ".fbx" => ImportWithAssimp(path),
            _ => throw new NotSupportedException($"Unsupported model format: {extension}")
        };
    }

    private static Model3DGroup ImportWithAssimp(string path)
    {
        using var context = new AssimpContext();
        var scene = context.ImportFile(
            path,
            PostProcessSteps.Triangulate |
            PostProcessSteps.GenerateSmoothNormals |
            PostProcessSteps.JoinIdenticalVertices |
            PostProcessSteps.ImproveCacheLocality |
            PostProcessSteps.PreTransformVertices);

        if (scene is null || scene.MeshCount == 0)
        {
            throw new InvalidOperationException("Imported scene does not contain any meshes.");
        }

        var group = new Model3DGroup();
        for (var i = 0; i < scene.MeshCount; i++)
        {
            group.Children.Add(ConvertMesh(scene.Meshes[i], scene.Materials));
        }

        return group;
    }

    private static GeometryModel3D ConvertMesh(Mesh mesh, IList<Assimp.Material> materials)
    {
        var geometry = new MeshGeometry3D();
        for (var i = 0; i < mesh.VertexCount; i++)
        {
            var vertex = mesh.Vertices[i];
            geometry.Positions.Add(new Point3D(vertex.X, -vertex.Y, vertex.Z));

            if (mesh.HasNormals)
            {
                var normal = mesh.Normals[i];
                geometry.Normals.Add(new System.Windows.Media.Media3D.Vector3D(normal.X, -normal.Y, normal.Z));
            }
        }

        for (var i = 0; i < mesh.FaceCount; i++)
        {
            var face = mesh.Faces[i];
            if (face.IndexCount != 3)
            {
                continue;
            }

            geometry.TriangleIndices.Add(face.Indices[0]);
            geometry.TriangleIndices.Add(face.Indices[1]);
            geometry.TriangleIndices.Add(face.Indices[2]);
        }

        var material = CreateMaterial(mesh.MaterialIndex, materials);
        return new GeometryModel3D(geometry, material) { BackMaterial = material };
    }

    private static System.Windows.Media.Media3D.Material CreateMaterial(int materialIndex, IList<Assimp.Material> materials)
    {
        if (materialIndex < 0 || materialIndex >= materials.Count)
        {
            return new DiffuseMaterial(new SolidColorBrush(Color.FromRgb(190, 190, 190)));
        }

        var source = materials[materialIndex];
        var diffuse = source.HasColorDiffuse
            ? ToWpfColor(source.ColorDiffuse)
            : Color.FromRgb(190, 190, 190);

        var emissive = source.HasColorEmissive
            ? ToWpfColor(source.ColorEmissive)
            : Color.FromArgb(0, diffuse.R, diffuse.G, diffuse.B);

        var material = new MaterialGroup();
        material.Children.Add(new DiffuseMaterial(new SolidColorBrush(diffuse)));
        if (emissive.A > 0)
        {
            material.Children.Add(new EmissiveMaterial(new SolidColorBrush(emissive)));
        }

        return material;
    }

    private static Model3DGroup Normalize(Model3DGroup model, double targetSize)
    {
        var clone = model.Clone();
        var bounds = clone.Bounds;
        if (bounds.IsEmpty)
        {
            throw new InvalidOperationException("Imported model does not have valid bounds.");
        }

        var collisionFootprint = Math.Max(bounds.SizeX, bounds.SizeY);
        if (collisionFootprint <= double.Epsilon)
        {
            throw new InvalidOperationException("Imported model has zero size.");
        }

        var center = new Point3D(
            bounds.X + (bounds.SizeX * 0.5),
            bounds.Y + (bounds.SizeY * 0.5),
            bounds.Z + (bounds.SizeZ * 0.5));
        var scale = targetSize / collisionFootprint;

        var transforms = new Transform3DGroup();
        transforms.Children.Add(new TranslateTransform3D(-center.X, -center.Y, -center.Z));
        transforms.Children.Add(new ScaleTransform3D(scale, scale, scale));
        clone.Transform = transforms;
        return clone;
    }

    private static Color ToWpfColor(Color4D color)
    {
        return Color.FromArgb(
            ToByte(color.A),
            ToByte(color.R),
            ToByte(color.G),
            ToByte(color.B));
    }

    private static byte ToByte(float value)
    {
        return (byte)Math.Clamp(Math.Round(value * 255.0f), 0, 255);
    }

    private static void ApplyTint(Model3D model, Color tint)
    {
        if (tint == Colors.White)
        {
            return;
        }

        switch (model)
        {
            case Model3DGroup group:
                for (var i = 0; i < group.Children.Count; i++)
                {
                    ApplyTint(group.Children[i], tint);
                }

                break;
            case GeometryModel3D geometry:
                geometry.Material = TintMaterial(geometry.Material, tint);
                geometry.BackMaterial = TintMaterial(geometry.BackMaterial, tint);
                break;
        }
    }

    private static System.Windows.Media.Media3D.Material? TintMaterial(System.Windows.Media.Media3D.Material? material, Color tint)
    {
        return material switch
        {
            DiffuseMaterial diffuse => new DiffuseMaterial(TintBrush(diffuse.Brush, tint)),
            EmissiveMaterial emissive => new EmissiveMaterial(TintBrush(emissive.Brush, tint)),
            MaterialGroup group => TintMaterialGroup(group, tint),
            _ => material
        };
    }

    private static MaterialGroup TintMaterialGroup(MaterialGroup source, Color tint)
    {
        var group = new MaterialGroup();
        for (var i = 0; i < source.Children.Count; i++)
        {
            var tinted = TintMaterial(source.Children[i], tint);
            if (tinted is not null)
            {
                group.Children.Add(tinted);
            }
        }

        return group;
    }

    private static Brush TintBrush(Brush brush, Color tint)
    {
        return brush switch
        {
            SolidColorBrush solid => new SolidColorBrush(MultiplyColor(solid.Color, tint)),
            _ => brush.Clone()
        };
    }

    private static Color MultiplyColor(Color left, Color right)
    {
        return Color.FromArgb(
            left.A,
            MultiplyByte(left.R, right.R),
            MultiplyByte(left.G, right.G),
            MultiplyByte(left.B, right.B));
    }

    private static byte MultiplyByte(byte left, byte right)
    {
        return (byte)((left * right) / 255);
    }
}
