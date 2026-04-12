using Google.Protobuf;

namespace LogRipper.Cli;

internal static class JsonOutput
{
    private static readonly JsonFormatter Formatter = new(JsonFormatter.Settings.Default.WithIndentation());

    public static void Print(IMessage message)
    {
        Console.WriteLine(Formatter.Format(message));
    }

    public static void PrintArray(IReadOnlyList<IMessage> messages)
    {
        Console.Write("[");

        for (var i = 0; i < messages.Count; i++)
        {
            if (i > 0)
            {
                Console.Write(",");
            }

            Console.WriteLine();
            Console.Write(Formatter.Format(messages[i]));
        }

        if (messages.Count > 0)
        {
            Console.WriteLine();
        }

        Console.WriteLine("]");
    }
}
