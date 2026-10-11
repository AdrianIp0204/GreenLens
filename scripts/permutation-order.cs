using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text;

namespace GreenLensPermutation {
    // Only the declared outer sensor ordering; this never changes engine sampling.
    public static class Order {
        static readonly byte[] Domain = Encoding.ASCII.GetBytes("GreenLens/reference-per-round-permutation/v1\0");
        public static bool Accept(ulong word, int bound) {
            if (bound < 2 || bound > 24) throw new ArgumentOutOfRangeException(nameof(bound));
            ulong b = (ulong)bound;
            // 2^64 % b without overflowing a u64 when the ceiling is exactly 2^64.
            ulong remainder = unchecked(0UL - b) % b;
            return remainder == 0 || word < ulong.MaxValue - remainder + 1;
        }
        static int[] Shuffle(Func<ulong> next) {
            int[] p = new int[24]; for (int i=0;i<24;i++) p[i]=i;
            for (int i=23;i>=1;i--) {
                ulong word; do { word=next(); } while (!Accept(word,i+1));
                int j=(int)(word%(ulong)(i+1)); int value=p[i];p[i]=p[j];p[j]=value;
            }
            return p;
        }
        public static int[] Round(int seed, int round) {
            if (seed<0 || seed>1 || round<0 || round>=512) throw new ArgumentOutOfRangeException();
            byte[] message=new byte[Domain.Length+24]; Domain.CopyTo(message,0);
            BinaryPrimitives.WriteUInt64LittleEndian(message.AsSpan(Domain.Length,8),(ulong)seed);
            BinaryPrimitives.WriteUInt64LittleEndian(message.AsSpan(Domain.Length+8,8),(ulong)round);
            ulong counter=0;
            return Shuffle(()=> {
                BinaryPrimitives.WriteUInt64LittleEndian(message.AsSpan(Domain.Length+16,8),counter++);
                return BinaryPrimitives.ReadUInt64LittleEndian(SHA256.HashData(message).AsSpan(0,8));
            });
        }
        // Pure seam verifies rejection consumption without hoping for a rare SHA draw.
        public static int[] FromWords(ulong[] words, out int consumed) {
            int n=0;
            int[] result=Shuffle(()=> {if(n>=words.Length) throw new ArgumentException("Insufficient draws");return words[n++];});
            consumed=n;return result;
        }
    }
}
