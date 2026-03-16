namespace ScreenOverlayPhysics.Models;

public sealed class AppConfig
{
    public float GravityY { get; set; } = 1800f;
    public float ThrowSensitivity { get; set; } = 1.1f;
    public float MaxThrowSpeed { get; set; } = 2600f;
    public float Restitution { get; set; } = 0.75f;
    public float LinearDamping { get; set; } = 0.992f;
    public float SleepThreshold { get; set; } = 24f;
    public float FloorSnapThreshold { get; set; } = 3f;
    public int InteractionDebounceMs { get; set; } = 80;
    public bool StartInPassThrough { get; set; } = true;
}
