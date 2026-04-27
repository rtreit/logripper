using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Tests;

/// <summary>
/// F7 (ResetTimer) is the operator's "starting a new QSO" signal. In
/// addition to resetting the on-screen elapsed timer, it must invoke
/// the attached CW reset-lock handler so the cw-decoder drops the
/// previous contact's pitch lock and re-acquires for the new station.
/// </summary>
public sealed class QsoLoggerResetTimerTests
{
    [Fact]
    public void AcknowledgeQsoStartCommandInvokesAttachedCwResetLockHandler()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());
        var calls = 0;
        logger.AttachCwResetLockHandler(() => calls++);

        logger.AcknowledgeQsoStartCommand.Execute(null);

        Assert.Equal(1, calls);
    }

    [Fact]
    public void AcknowledgeQsoStartCommandIsSafeWhenNoCwResetLockHandlerIsAttached()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());

        // Should not throw — handler is null when the cw-decoder is
        // disabled or unavailable.
        logger.AcknowledgeQsoStartCommand.Execute(null);

        Assert.Equal("00:00", logger.ElapsedTimeText);
    }

    [Fact]
    public void AcknowledgeQsoStartCommandSwallowsHandlerExceptionsSoF7NeverDeadlocks()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());
        logger.AttachCwResetLockHandler(() => throw new InvalidOperationException("boom"));

        // F7 must keep working even if the cw-decoder pipe write throws.
        logger.AcknowledgeQsoStartCommand.Execute(null);

        Assert.Equal("00:00", logger.ElapsedTimeText);
    }
}
