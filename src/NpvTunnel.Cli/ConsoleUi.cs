// Console presentation boundary. Every command writes through this class,
// so the CLI has exactly one file that touches Console directly and the
// JSON output stays a stable contract.

using System.Text.Json;
using System.Text.Json.Nodes;

namespace NpvTunnel.Cli;

/// <summary>Console output helpers (JSON and human modes). The writers are
/// captured once here - this is the single presentation boundary.</summary>
public static class ConsoleUi
{
    private static readonly TextWriter Out = Console.Out;
    private static readonly TextWriter Err = Console.Error;

    /// <summary>Write a plain line to stdout.</summary>
    public static void Line(string text) => Out.WriteLine(text);

    /// <summary>Write an empty line to stdout.</summary>
    public static void Blank() => Out.WriteLine();

    /// <summary>Write a warning line to stderr.</summary>
    public static void Warn(string text) => Err.WriteLine(text);

    /// <summary>Write an error line to stderr.</summary>
    public static void Error(string text) => Err.WriteLine(text);

    /// <summary>Emit a JSON value on stdout.</summary>
    public static void Json(JsonNode value, bool pretty)
    {
        var options = new JsonSerializerOptions { WriteIndented = pretty };
        Out.WriteLine(value.ToJsonString(options));
    }

    /// <summary>Build a JSON object from key/value pairs in order.</summary>
    public static JsonObject Obj(params (string Key, JsonNode? Value)[] fields)
    {
        var obj = new JsonObject();
        foreach (var (key, value) in fields)
        {
            obj[key] = value;
        }

        return obj;
    }

    /// <summary>JSON string node (implicit conversion, never null).</summary>
    public static JsonNode S(string value) => value;

    /// <summary>JSON number node.</summary>
    public static JsonNode N(long value) => value;

    /// <summary>JSON boolean node.</summary>
    public static JsonNode B(bool value) => value;
}
