// Original GreenLens research helper, Apache-2.0. No production dependency.
using System;
using System.Globalization;
using System.IO;
using System.Text;
namespace GreenLensPairedGrid {
public static class Primary {
    public static void WriteRays(string sensorText, int side, string outputPath) {
        if (side != 128 && side != 256) throw new ArgumentException("Unsealed primary side");
        using var text = new StreamWriter(outputPath, false, new UTF8Encoding(false), 65536);
        var culture = CultureInfo.InvariantCulture;
        var lines = sensorText.Trim().Split('\n');
        if (lines.Length != 24) throw new ArgumentException("Expected24 sensors");
        foreach (var line in lines) {
            var tokens = line.Split((char[])null, StringSplitOptions.RemoveEmptyEntries);
            if (tokens.Length != 6) throw new ArgumentException("Invalid sensor record");
            var p = new double[6];
            for (int k = 0; k < 6; k++) {
                p[k] = double.Parse(tokens[k], culture);
                if (!double.IsFinite(p[k])) throw new ArgumentException("Nonfinite sensor");
            }
            if (p[3] != 0 || p[4] != 0 || p[5] != 1) throw new ArgumentException("Upward sensors required");
            string origin = p[0].ToString("G17", culture)+" "+p[1].ToString("G17", culture)+" "+(p[2]+.0001).ToString("G17", culture)+" ";
            for (int radial = 0; radial < side; radial++) {
                double radius = Math.Sqrt((radial+.5)/side);
                double z = Math.Sqrt(1-radius*radius);
                for (int azimuth = 0; azimuth < side; azimuth++) {
                    double phi = 2*Math.PI*(azimuth+.5)/side;
                    text.Write(origin);
                    text.Write((radius*Math.Cos(phi)).ToString("G17", culture)); text.Write(' ');
                    text.Write((radius*Math.Sin(phi)).ToString("G17", culture)); text.Write(' ');
                    text.Write(z.ToString("G17", culture)); text.Write('\n');
                }
            }
        }
    }
}
}
