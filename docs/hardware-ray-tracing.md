# The card's own ray tracing behind the title's traversal

Branch `rt-hardware`. Status, 2026-11-27: measured that it is worth doing and
that the card can; nothing of it runs in the title yet.

## Why

The title traces rays with compute shaders that walk its own structures and
call one hardware instruction a node (`docs/fork-ray-tracing.md`). Here that
instruction is emulated in DXIL and every node is read through the directory
of memory by address. Two timing runs of the same scene on the RX 7700 XT
(2026-11-26, Series S profile):

| trace program | with structures (tm-rt) | every ray missing at once (tm-nort) |
|---|---|---|
| f22a76fcb967ee59 | 7.93 ms | 0.04 ms |
| 6d7fe7745ab9c6d8 (twice a frame) | 6.85 | 0.03 |
| 3592426e3e9142d8 | 1.73 | 0.02 |
| b58eb3ca6ee46427 | 1.40 | 0.00 |
| 36f04a4ed0ad1d9b | 1.29 | 0.01 |
| a1b20bc47e3c9597 | 0.69 | 0.02 |

So of the 20.4 ms the trace programs take, all but a tenth of a millisecond
is the walk. (The structure builder is 5 ms more and is not touched by this.)

## What the card does

`host::hwrt` (an ignored test): DXR tier 1.1, inline ray queries from a
compute shader, rays from random points in random directions to the first
hit, through one bottom-level structure of small triangles strewn in a cube
(the worst case for a structure: nothing is coherent):

| triangles | structure | built in | rays a second |
|---|---|---|---|
| 100,000 | 15 MB | 6 ms | 410-610 million |
| 1,000,000 | 155 MB | 70 ms | 240-255 million |
| 4,000,000 | 620 MB | 288 ms | 130 million |

A million rays in 2.6 to 8 ms. How many rays the title's programs trace a
frame is not measured yet (their dispatches are indirect).

## The way in

The title's loops stay. A structure of the host's is made for each of the
title's bottom-level structures, from the triangles in its leaf nodes (a
triangle node holds up to four triangles of five vertices,
`victory_shader::bvh`). Where a translated program tests a ray against the
root of such a structure, it asks the card's structure instead (one ray
query) and answers as the instruction would for a box node whose only
children the ray enters are the triangle nodes of the nearest hits: the loop
then tests those triangles itself, with the title's own triangle test, and
visits no box node. What a ray costs falls from some tens of node tests,
each with its loads, to a query and a few triangle tests.

Open, in the order they decide whether this works:

1. Where a bottom-level structure's leaves are and how many triangles each
   holds, from the header (`fork-ray-tracing.md`: dword 13 the elements'
   offset, 3 their number), checked against a dump of the arenas.
2. The structures are whole only on the card (shipped packed, unpacked there
   by the builder; dynamic ones built there): the vertices must be gathered
   by a pass on the card after the builder's, then the host's structure
   built or refitted from them.
3. A ray query in translated DXIL (the emitter has none; test shaders come
   from the SDK's dxc), with the structure picked by the title structure's
   address.
4. What the title does with hits it turns down (alpha tested leaves, its own
   minimum distance): the answer must hold the next hits too, up to the four
   a box node can name.
5. Memory: the host's structures are 150 MB a million triangles before
   compaction, on a card that is at its budget with ray tracing.
6. Then the top level the same way (one query for the instance).
