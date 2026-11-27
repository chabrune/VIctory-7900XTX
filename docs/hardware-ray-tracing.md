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

## What the title's structures hold (a dump of the arenas, run bld1)

The first played scene, 2026-11-27, ray tracing low: the main top level has
1,590 instances of 520 bottom-level structures (a second one 302 of 90).
Those 520 hold 603,685 triangles in all (1.79 million counted once an
instance; the largest 47,877, most a few hundred), under 210,834 box nodes
of 128 bytes and 31,068 of 64, up to 15 deep.

**Every triangle is a node of its own, in one stretch.** A bottom level's
header gives the offset of its elements (dword 13) and their number (dword
3); element `i` is the 64-byte node at that offset plus 64 i, its triangle
the node's first three vertices (bytes 0-35), named by the pointer
`(offset / 64 + i) << 3` of kind 0. Of 603,673 triangles reached from the
roots, 603,626 are so; the rest (kinds 1-3, or outside the stretch) belong
to the few dynamic structures that are written over each other
(`fork-ray-tracing.md`, known gaps).

An instance is 128 bytes at the top level's elements: the matrix in dwords
0-11, dword 14 the bottom level's root pointer with flags in its high bits,
dword 15 the bottom level's address in 256-byte units.

**So the card's structure can be built from the title's leaves where they
lie**, with no pass to gather vertices: vertex stride 4 bytes (any multiple
of a component is allowed), 32-bit indices 16 i, 16 i + 3, 16 i + 6.
`host::hwrt` does it both ways: the same hits to the last one, the same
speed, twice the build time (7.6 ms for 100,000 triangles, 130 ms for a
million). The primitive a ray query returns is `i`: the title's node pointer
follows by arithmetic. For 600,000 triangles the card's structures take
about 95 MB before compaction.

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

## Done so far

- The translator's side (`VICTORY_RT_QUERY=1`, off by default; commit
  a14c71c3): a box test of a node in title memory looks the node's address
  up in a table that follows the directory of memory by address
  (`shader::RT_ROOTS`: 16,384 entries of four dwords, one probe); for a root
  the host has a structure for, `Lower::bvh_root` emits an inline ray query
  (structure from the view heap at `base + RT_FIRST_SLOT + n`, every
  triangle opaque, the ray's own origin, direction and extent) and returns
  the nearest triangle's node as the box's one child. The DXIL validates.
  Nothing fills the table yet, so nothing changes in a run.

## In the title, 2026-11-27 (RX 7700 XT, Series S profile, the opening)

Both sides run (`VICTORY_RT_QUERY=1`; `memory/rt.rs` is the host's): some
900 structures of the host's alive in the first played scene, 600,000
triangles, about 100 MB, built over the first seconds of the world; no
device loss in six runs of 130-150 s, the room lit as without the switch
(captures of different moments: not yet the same frame both ways).

With the trace programs in every frame (`VICTORY_TRACE_EVERY=1`), one run
each under the lock that keeps two games apart (hw14 without, hw13 with):

| scene | without | with |
|---|---|---|
| 4,650-4,820 draws | 33.2 ms | 35.1 ms |
| 5,600-6,800 draws (the heavy played scene) | 77-92 ms, the card waited for 39-52 ms | 50-54 ms, 10-14 ms |

Per program the timing runs are too noisy to say more than this (a trace
runs once in five frames there, and the cutscene moves): the largest
(f22a76fcb967ee59) went from 4-11 ms a run to 2-7 in the played scene.

What it does not do yet, and why the gain is not the 20 ms of the table at
the top: the top level is still walked with the emulated test, a query is
made for every instance whose root's boxes the ray enters (asked without
that test first, the programs with long rays got slower: 3592426e3e9142d8
1.9 ms to 4.1), every box node pays the look in the roots' table, and the
structures that are not unpacked static ones (a tenth of the triangles) are
walked as before.

## The whole scene in one query (`VICTORY_RT_SCENE=1`, written 2026-11-27)

Beyond the roots of bottom levels, the step that would carry the real gain:
one query for the ray at a **top level**, over every instance of the scene.
The host reads a packed structure's 64-byte header when it comes, takes it
for a top level when it is version 6, not a bottom level and not packed
(`marks & 0x8100 == 0`), reads its instances (128 bytes each: three rows of
four floats, the bottom level's address in dword 15 in 256-byte units) and
builds **one structure of the host's over the instances it has one of ours
for**, with the title's matrices and the title's instance number as the
card's `InstanceID`.

A trace program that comes to the top level's root now reads an entry of
the roots' table marked [`shader::RT_ROOT_SCENE`] and asks the card once;
the answer is the title's own instance node (kind 6) of the nearest hit,
`(first + 2 instance) << 3 | 6`, so the program descends into that instance
and transforms its ray as it always did. The entry counts only once the
build was submitted and two frames have passed, and its view is in the heap
before then. The structure is built again when the set of instances the
host has a structure for changes (at most once a frame, while the static
structures stream in over the world's first seconds).

What is assumed and not measured: that a title instance is 128 bytes at the
top level's elements offset (dword 13), that its three rows of four floats
are DXR's own `Transform` rows, that its 128-byte node's pointer is
`(elements/64 + 2 i) << 3 | 6`, and that a top level's header keeps no 0x100
mark once unpacked. Written behind `VICTORY_RT_SCENE=1` (off by default,
needs `VICTORY_RT_QUERY=1`), **not compiled and not run** as of 2026-11-27
(the machine was in a run); the structures that are not the host's (dynamic
ones) are still walked as before, so the scene's structure covers what the
host had built by then.

## The host's side (`crates/victory-gpu/src/memory/rt.rs`)

- Which structures: the bottom levels a top-level build names in upload
  memory (`named_in_upload`), and the packed ones the module already
  follows (350 of the scene's 520 are in its `packed` map, with their going).
  First only those marked rehydrated (0x8002: 543,000 of the 617,000
  triangles); the compacted dynamic ones (0x8004, 64,000) once it is known
  whether they are refitted.
- The header (elements' offset and number) is whole only on the card: 64
  bytes copied to a read-back buffer by a list of ours, read a frame later,
  and asked again now and then (another structure at the address).
- The leaves are not always one stretch of an arena (374 structures of 413
  are; the 39 others hold a quarter of the triangles): they are copied page
  by page into a scratch buffer, and the structure built from that with the
  stride of 4 and the shared indices.
- The build is recorded in the title's own list in front of the first trace
  dispatch that comes after the header is known: the builder's unpacking
  stages are in earlier lists by the title's order.
- A top level of one instance for each, its view at the slot behind the
  memory copies' in every heap that has their run, then the root's entry in
  the table (the directory's memory, which goes up as it does today).

Open, in the order they decide whether this works:

1. (Settled, above: the leaves are one stretch, a triangle each.)
2. The structures are whole only on the card (shipped packed, unpacked there
   by the builder; dynamic ones built there): the host's structure is built
   on the card after the builder's stages, from the arena or the title
   buffer the leaves are in; the header (element count and offset) must be
   known to the CPU, read back once a structure is unpacked. When to build
   again: a dynamic structure the builder refits every frame.
3. A ray query in translated DXIL (the emitter has none; test shaders come
   from the SDK's dxc), with the structure picked by the title structure's
   address.
4. What the title does with hits it turns down (alpha tested leaves, its own
   minimum distance): the answer must hold the next hits too, up to the four
   a box node can name.
5. Memory: the host's structures are 150 MB a million triangles before
   compaction, on a card that is at its budget with ray tracing.
6. Then the top level the same way (one query for the instance).
