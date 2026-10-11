// Original GreenLens numerical-verification helper, Apache-2.0.
using System;
using System.IO;
using System.Globalization;
using System.Collections.Generic;

namespace GreenLensReference {
public sealed class SensorMean {
    public int sensor_index;
    public string sensor_id;
    public double[] diffuse_rgb, full_solar_rgb, centroid_direct_rgb, finite_direct_rgb, total_rgb;
    public double lux;
}
public sealed class Mean {
    public int first_round, rounds;
    public SensorMean[] sensors;
}
public sealed class Comparison {
    public int sensor_index;
    public double lower_lux, higher_lux, absolute_difference_lux, allowed_difference_lux;
    public bool passed;
}
public static class Aggregate {
    static readonly CultureInfo C = CultureInfo.InvariantCulture;
    public static double[] Read(string path, int expected) {
        var values = new double[expected]; int count = 0;
        using var reader = new StreamReader(path);
        string line;
        while ((line = reader.ReadLine()) != null) {
            foreach (var token in line.Split((char[])null, StringSplitOptions.RemoveEmptyEntries)) {
                if (count == expected) throw new InvalidDataException("Excess primary output");
                var value = double.Parse(token, C);
                if (!double.IsFinite(value) || value < 0) throw new InvalidDataException("Invalid primary output");
                values[count++] = value;
            }
        }
        if (count != expected) throw new InvalidDataException("Truncated primary output");
        return values;
    }
    public static void EqualPrefix(double[] shorter, double[] longer) {
        if (longer.Length < shorter.Length) throw new InvalidDataException("Shorter escalation output");
        for (int i = 0; i < shorter.Length; i++)
            if (shorter[i] != longer[i]) throw new InvalidDataException("Escalation does not reproduce the frozen deterministic prefix");
    }
    public static Mean Calculate(double[] values, int sensors, int firstRound, int rounds,
                                  double scale, double[][] direct, double[][] finite) {
        if (sensors < 1 || rounds < 1 || firstRound < 0 || !double.IsFinite(scale) || scale <= 0 ||
            values.Length < checked((firstRound+rounds)*sensors*6) || direct.Length != sensors || finite.Length != sensors)
            throw new InvalidDataException("Invalid averaging dimensions");
        var result = new Mean { first_round=firstRound, rounds=rounds, sensors=new SensorMean[sensors] };
        for (int sensor=0; sensor<sensors; sensor++) {
            var row = new SensorMean { sensor_index=sensor, sensor_id="s"+(sensor+1).ToString("D6",C),
                diffuse_rgb=new double[3], full_solar_rgb=new double[3],
                centroid_direct_rgb=(double[])direct[sensor].Clone(), finite_direct_rgb=(double[])finite[sensor].Clone(), total_rgb=new double[3] };
            for (int round=firstRound; round<firstRound+rounds; round++) {
                int offset=(round*sensors+sensor)*6;
                for (int channel=0; channel<3; channel++) {
                    row.diffuse_rgb[channel]+=values[offset+channel];
                    row.full_solar_rgb[channel]+=values[offset+3+channel];
                }
            }
            for (int channel=0; channel<3; channel++) {
                row.diffuse_rgb[channel]/=rounds*scale;
                row.full_solar_rgb[channel]/=rounds*scale;
                row.total_rgb[channel]=row.diffuse_rgb[channel]+row.full_solar_rgb[channel]-direct[sensor][channel]+finite[sensor][channel];
                if (!double.IsFinite(row.total_rgb[channel]) || row.total_rgb[channel]<0) throw new InvalidDataException("Invalid corrected reference RGB; no clamping");
            }
            row.lux=179*(.265074126*row.total_rgb[0]+.670114631*row.total_rgb[1]+.064811243*row.total_rgb[2]);
            result.sensors[sensor]=row;
        }
        return result;
    }
    public static Comparison[] Compare(Mean lower, Mean higher) {
        return CompareWithScale(lower,higher,higher);
    }
    public static Comparison[] CompareWithScale(Mean lower, Mean higher, Mean toleranceReference) {
        if (lower.sensors.Length!=higher.sensors.Length) throw new InvalidDataException("Comparison count differs");
        var result=new Comparison[lower.sensors.Length];
        for(int i=0;i<result.Length;i++) {
            double delta=Math.Abs(lower.sensors[i].lux-higher.sensors[i].lux), allowed=Math.Max(.1,.01*toleranceReference.sensors[i].lux);
            result[i]=new Comparison { sensor_index=i,lower_lux=lower.sensors[i].lux,higher_lux=higher.sensors[i].lux,
                absolute_difference_lux=delta,allowed_difference_lux=allowed,passed=delta<=allowed };
        }
        return result;
    }
}
}
