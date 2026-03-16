using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public interface ISceneObjectVisual3D
{
    ModelVisual3D Visual { get; }

    void Update(
        double centerX,
        double centerY,
        double centerZ,
        double rotationX,
        double rotationY,
        double rotationZ,
        double scaleX,
        double scaleY);
}
