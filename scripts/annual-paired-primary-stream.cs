// Original GreenLens research helper, Apache-2.0. No production dependency.
// One continuous sensor-major stream; no retained giant ray file and no reseeding.
using System;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace GreenLensPairedStream {
public sealed class Receipt {
    public long Bytes { get; set; }
    public long Rays { get; set; }
    public string Sha256 { get; set; }
}
public sealed class HammersleyProducer {
    readonly string[] origins;
    readonly int count;
    public int SensorCount { get { return origins.Length; } }
    public int PrimaryCount { get { return count; } }
    public long ExpectedRays { get { return (long)origins.Length * count; } }
    public HammersleyProducer(string sensorText, int count) {
        if (count != 16384 && count != 65536 && count != 262144)
            throw new ArgumentException("Unsealed Hammersley primary count");
        if (String.IsNullOrWhiteSpace(sensorText) || !sensorText.EndsWith("\n", StringComparison.Ordinal))
            throw new ArgumentException("Sensor records must end with a newline");
        var lines = sensorText.TrimEnd('\r', '\n').Split('\n');
        if (lines.Length < 1 || lines.Length > 256) throw new ArgumentException("Expected 1 through 256 sensors");
        origins = new string[lines.Length];
        var culture = CultureInfo.InvariantCulture;
        for (int j = 0; j < lines.Length; j++) {
            var tokens = lines[j].Split((char[])null, StringSplitOptions.RemoveEmptyEntries);
            if (tokens.Length != 6) throw new ArgumentException("Invalid sensor record");
            var p = new double[6];
            for (int k = 0; k < 6; k++) {
                p[k] = double.Parse(tokens[k], culture);
                if (!double.IsFinite(p[k])) throw new ArgumentException("Nonfinite sensor");
            }
            if (p[3] != 0 || p[4] != 0 || p[5] != 1) throw new ArgumentException("Upward sensors required");
            var z = p[2] + .0001;
            if (!double.IsFinite(z)) throw new ArgumentException("Nonfinite effective sensor origin");
            origins[j] = p[0].ToString("G17", culture)+" "+p[1].ToString("G17", culture)+" "+z.ToString("G17", culture)+" ";
        }
        this.count = count;
    }
    public static double RadicalInverse(int index) {
        if (index < 0) throw new ArgumentOutOfRangeException(nameof(index));
        double value = 0, fraction = .5;
        while (index != 0) { value += (index & 1) * fraction; fraction *= .5; index >>= 1; }
        return value;
    }
    public Task<Receipt> GenerateAsync(Stream destination, CancellationToken cancellation) {
        if (destination == null || !destination.CanWrite) throw new ArgumentException("Writable destination required");
        return Task.Run(async () => {
            var culture = CultureInfo.InvariantCulture;
            var buffer = new byte[65536];
            int used = 0;
            long bytes = 0, rays = 0;
            using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            foreach (var origin in origins) {
                for (int i = 0; i < count; i++) {
                    cancellation.ThrowIfCancellationRequested();
                    double radius = Math.Sqrt((i + .5) / count);
                    double phi = 2 * Math.PI * RadicalInverse(i);
                    var line = origin + (radius*Math.Cos(phi)).ToString("G17",culture)+" "+
                        (radius*Math.Sin(phi)).ToString("G17",culture)+" "+
                        Math.Sqrt(1-radius*radius).ToString("G17",culture)+"\n";
                    int length = Encoding.ASCII.GetByteCount(line);
                    if (length > buffer.Length) throw new InvalidDataException("Ray record exceeds buffer");
                    if (used + length > buffer.Length) {
                        await destination.WriteAsync(buffer,0,used,cancellation).ConfigureAwait(false);
                        hash.AppendData(buffer,0,used); bytes += used; used = 0;
                    }
                    used += Encoding.ASCII.GetBytes(line,0,line.Length,buffer,used);
                    rays++;
                }
            }
            if (used != 0) {
                await destination.WriteAsync(buffer,0,used,cancellation).ConfigureAwait(false);
                hash.AppendData(buffer,0,used); bytes += used;
            }
            await destination.FlushAsync(cancellation).ConfigureAwait(false);
            return new Receipt { Bytes=bytes, Rays=rays,
                Sha256=Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant() };
        }, cancellation);
    }
}
}
