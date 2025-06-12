#!/bin/bash
# filepath: check-elf-simd.sh

BINARY="$1"

if [[ ! -f "$BINARY" ]]; then
    echo "Usage: $0 <elf_binary>"
    exit 1
fi

echo "=== SIMD Analysis for $BINARY ==="

# Check if it's an ELF binary
if ! file "$BINARY" | grep -q "ELF"; then
    echo "Error: Not an ELF binary"
    exit 1
fi

# Check architecture
ARCH=$(file "$BINARY" | grep -o "x86-64\|i386")
echo "Architecture: $ARCH"

if [[ "$ARCH" != "x86-64" ]]; then
    echo "Warning: This script is designed for x86_64 binaries"
fi

echo -e "\n=== Checking for SIMD Instructions ==="

# SSE instructions
SSE_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(movaps|movups|addps|subps|mulps|divps|movss|addss|movlps|movhps|shufps|unpcklps|unpckhps)")
echo "SSE instructions: $SSE_COUNT"

# SSE2 instructions  
SSE2_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(movapd|movupd|addpd|subpd|mulpd|divpd|movsd|addsd|movdqu|movdqa|paddd|psubd|pmuludq|pshufd|punpck)")
echo "SSE2 instructions: $SSE2_COUNT"

# SSE3 instructions
SSE3_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(addsubps|addsubpd|haddps|haddpd|hsubps|hsubpd|movshdup|movsldup|movddup|lddqu)")
echo "SSE3 instructions: $SSE3_COUNT"

# SSSE3 instructions
SSSE3_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(pabsb|pabsw|pabsd|psignb|psignw|psignd|pshufb|pmulhrsw|pmaddubsw|phaddw|phaddd|phsubw|phsubd)")
echo "SSSE3 instructions: $SSSE3_COUNT"

# SSE4.1 instructions
SSE41_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(pblendvb|pblendw|pmulld|pmuldq|dpps|dppd|mpsadbw|phminposuw|pminsb|pmaxsb|pminuw|pmaxuw|pminud|pmaxud|pminsd|pmaxsd|roundps|roundpd|roundss|roundsd|ptest|pmovsxbw|pmovzxbw|packusdw)")
echo "SSE4.1 instructions: $SSE41_COUNT"

# SSE4.2 instructions
SSE42_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(pcmpestri|pcmpestrm|pcmpistri|pcmpistrm|pcmpgtq|crc32)")
echo "SSE4.2 instructions: $SSE42_COUNT"

# AVX instructions
AVX_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(vmovaps|vmovups|vaddps|vsubps|vmulps|vdivps|vxorps|vandps|vorps|vmovapd|vmovupd|vaddpd|vsubpd|vmulpd|vdivpd)")
echo "AVX instructions: $AVX_COUNT"

# AVX2 instructions
AVX2_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(vpaddd|vpsubd|vpmulld|vpand|vpor|vpxor|vbroadcast|vperm2i128|vextracti128|vinserti128|vgather)")
echo "AVX2 instructions: $AVX2_COUNT"

# AVX-512 instructions (zmm registers and mask registers)
AVX512_COUNT=$(objdump -d "$BINARY" 2>/dev/null | grep -c -E "(zmm[0-9]+|%k[0-7]|vaddps.*zmm|vmovaps.*zmm)")
echo "AVX-512 instructions: $AVX512_COUNT"

TOTAL_SIMD=$((SSE_COUNT + SSE2_COUNT + SSE3_COUNT + SSSE3_COUNT + SSE41_COUNT + SSE42_COUNT + AVX_COUNT + AVX2_COUNT + AVX512_COUNT))

echo -e "\nTotal SIMD instructions: $TOTAL_SIMD"

if [[ $TOTAL_SIMD -gt 0 ]]; then
    echo -e "\n=== Sample SIMD Instructions ==="
    objdump -d "$BINARY" 2>/dev/null | grep -E "(movaps|movups|vmov|vadd|vpadd|pshufb|pmulld|pcmp)" | head -8
    
    echo -e "\n=== SIMD Register Usage ==="
    objdump -d "$BINARY" 2>/dev/null | grep -o -E "%(xmm|ymm|zmm)[0-9]+" | sort | uniq -c | sort -nr
    
    echo -e "\n=== SIMD Instruction Breakdown ==="
    if [[ $SSE_COUNT -gt 0 ]]; then echo "  SSE: $SSE_COUNT instructions"; fi
    if [[ $SSE2_COUNT -gt 0 ]]; then echo "  SSE2: $SSE2_COUNT instructions"; fi
    if [[ $SSE3_COUNT -gt 0 ]]; then echo "  SSE3: $SSE3_COUNT instructions"; fi
    if [[ $SSSE3_COUNT -gt 0 ]]; then echo "  SSSE3: $SSSE3_COUNT instructions"; fi
    if [[ $SSE41_COUNT -gt 0 ]]; then echo "  SSE4.1: $SSE41_COUNT instructions"; fi
    if [[ $SSE42_COUNT -gt 0 ]]; then echo "  SSE4.2: $SSE42_COUNT instructions"; fi
    if [[ $AVX_COUNT -gt 0 ]]; then echo "  AVX: $AVX_COUNT instructions"; fi
    if [[ $AVX2_COUNT -gt 0 ]]; then echo "  AVX2: $AVX2_COUNT instructions"; fi
    if [[ $AVX512_COUNT -gt 0 ]]; then echo "  AVX-512: $AVX512_COUNT instructions"; fi
fi

echo -e "\n=== Checking Compiler Optimizations ==="
if strings "$BINARY" | grep -q "GCC\|clang"; then
    echo "Compiler info found in binary"
    strings "$BINARY" | grep -E "(GCC|clang)" | head -3
fi

# Check for auto-vectorization hints
if objdump -d "$BINARY" 2>/dev/null | grep -q -E "(loop|vector)"; then
    echo "Possible vectorized loops detected"
fi

echo -e "\n=== CPU Feature Detection ==="
echo "Available CPU SIMD features on this system:"
grep -o -E "(sse|sse2|sse3|ssse3|sse4_1|sse4_2|avx|avx2|avx512)" /proc/cpuinfo | tr ' ' '\n' | sort | uniq
