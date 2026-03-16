using System;
using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class ImportedModelVisual3D : SceneObjectVisual3DBase
{
    public ImportedModelVisual3D(string sourcePath, double size, System.Windows.Media.Color tint)
        : base(BuildModel(sourcePath, size, tint))
    {
        if (string.IsNullOrWhiteSpace(sourcePath))
        {
            throw new ArgumentException("A model source path is required.", nameof(sourcePath));
        }
    }

    private static Model3DGroup BuildModel(string sourcePath, double size, Color tint)
    {
        var normalized = ModelImportService.LoadNormalized(sourcePath, 1.0, tint);
        var group = new Model3DGroup();
        group.Children.Add(normalized);
        group.Transform = new ScaleTransform3D(size, size, size);
        return group;
    }
}
