# What was learned on the RX 7900 XTX (2026-11-23 to 2026-11-25)

One PC: i9-12900K (8 fast cores, 8 slow ones, 24 threads), 64 GB, Radeon RX 7900 XTX
24 GB, Windows 11, 1440p, Series X profile. Figures are of diagnostics builds with a
screenshot a second. Scenes are not the same from run to run, so read differences
under about 5 % as nothing. What a switch does is in the comment where it is read.

## Where a frame goes

Median of whole runs, same part of the prologue: 280 ms, then 124, 103, 75, now 46-65.
On the water (first boat mission) 46-53 ms.

The title needs 33 ms. Of a 55 ms frame on the water:

- **The thread that prepares submissions: 48 ms.** It is the wall. Steps of the lists
  18 (refreshes of host copies 10), steps done on the card 7, waiting for descriptor
  records 6, asking the write watch 5, copies in the card's memory 4, the title's bytes
  under shader writes 4.
- **The card: about 41 ms busy**, the next wall.
- **List calls: 36,000 a frame, 230 ms of work** spread over eight threads. In a street
  chase 67,000 calls and 430 ms.
- Descriptor records copied as plain bytes: 400,000 slots looked at, 40,000 mirrored a
  frame, most of them empty.
- Landing what shaders wrote: 16 MB a frame put as dwords, on a thread of its own.

## The one idea behind most of it

On the console the CPU and the graphics processor share memory. Here they do not, so
everything one writes and the other reads must be noticed and moved. Every large cost
is one of four: **noticing** writes of the title (write watch, guards), **noticing**
writes of shaders (hashes on the card, compares on the CPU), **moving** them, and
**waiting** until an order is safe. Gains came from making each of them narrower
(dwords, not pages), wider (threads, AVX2) or later (on the card, behind the frame),
in that order of size:

| What | Frame before | after |
|---|---|---|
| Descriptor records of draws' tables left to the draws | 280 ms | about 124 |
| Our hash passes over card copies, 64 threads a piece | card 79 ms | 66 |
| Preparing on a thread of its own, refreshes and records spread | 103 | 75 |
| List calls written down and translated by eight threads (`VICTORY_THIN`) | 75 | 68 |
| Submissions joined, card steps, textures sent ahead | 68 | about 63 |
| Title bytes under shader writes compared with AVX2 on six threads | 14 ms of it | 4-7 |
| Stores into guarded memory looked at first, fewer guards reset | 69 ms of threads' time | about 6 |

(Each figure is from the run in which it was measured; they overlap and do not add up.)

## Faults found, and what they were

- **Boats sank, the sea plane went under.** The ocean's height and displacement maps are
  textures that compute passes write and the title's physics sample on the CPU. They were
  never brought back to title memory. Fixed by Geckomacabre's read-back as merged in
  fxceless/victory-fork f5c03f3b; on by default here (`VICTORY_TEXTURE_BACK=0`: off).
  With it the boat stays up through a whole run; without it sank in every one.
- **A device loss about every 16 minutes**: a page fault inside a texture made in the same
  tenth of a second. A slot showed it before its upload had run.
  `VICTORY_SHOW_AFTER=1`; none in the sessions since (likely fixed, not proven).
- **A rare lost write** (6 of 200 runs of a stress test): a view was mapped between our
  look at its guards and our lock, and views were opened one after another. None in 603
  runs after.
- **Blocky patches**: indirect dispatches cut at 65,535 groups.
- **Square holes and dashed edges in shadows**: pixel shaders that read their quad's
  neighbours with wave reads; on AMD helper lanes take no part in them.
- **Cube reflections too blurred, wrong materials**: DXIL's `Log` is base 2, and
  normalized buffer codes were one step off.
- **Every ray missed**: the dispatch flag that says counts are threads, not groups.

## Measured and not worth it

- Unordered early pipelines for everything (AMD's compiler crashed).
- Series S profile, smaller textures, a second drawing queue, `ASYNC_SUBMIT` off.
- Mapping pages on sixteen threads: no more pages mapped, frames longer.
- Scalar buffer loads four dwords a call (`VICTORY_SHADER_OPT=127`): no gain on this card.
- Wave sizes, fused multiply-adds, fast reciprocals, raw picked buffers, leaving out
  barriers: nothing to 0.1 ms each. Exact RDNA float rules (`VICTORY_RDNA_FLOAT`): 0.8 ms.
- Running the console's shader code natively on the card: the Windows driver takes no raw
  code, the two generations differ, and a Vulkan host layer ends at the same compiler. What
  is left to gain from shader code was guessed at under 1 ms.
- An on-screen panel of numbers (`VICTORY_PANEL`): it helps finding where a frame goes but
  costs some, so it is off and F3 shows it.

## Pipelines the title has ready-made (device methods 0x1c8 and 0x218), 2026-11-26

Read on an RX 7700 XT, Series S profile, from 96 descriptions of each method written
down with `VICTORY_PIPELINE_DUMP=<folder>` (a diagnostics build; the folder outside the
repository). In 47 s of the opening the title asks 0x1c8 for 10,277 different pipelines
and 0x218 for 2,690.

**What they are.** Pipelines whose programs the console's compiler made ahead of time,
handed over in pieces instead of a shader container. The pieces are those of the `FSPX`
that an `ACCC` part holds (`victory_shader::native::fspx_program`). When a method
answers that it is not there, the title makes the same pipeline from containers, which
is how every run so far drew: these calls are not missing pictures.

**0x218, compute**, 12 words of 8 bytes: the root signature object; then four pieces as
pointer and length: the pipeline's registers (0x1c bytes: thread-group size as three
u16 at +0x0c, flags at +0x12, `COMPUTE_PGM_RSRC1` and `2` at +0x14 and +0x18), a note
about the pipeline (0x14 bytes, a hash first), the GFX10 code, the program's metadata
(0x403, size - 0x14, code dwords, then 32-byte records 0x41/0x42/0x43 from +0x18); then
three words of the title's own (two descriptions it is linked with, a hash).

**0x1c8, graphics**, 13 words: the root signature object, a block of registers (0x17c,
0x184 or 0x18c bytes), a note about the pipeline (0x78 to 0xbc bytes, with the input
layout: element count, then semantic names such as POSITION, COLOR, TEXCOORD and
formats), then code and metadata of the pixel program (null for depth-only pipelines)
and of the vertex program. A stage's metadata: u32 2, size - 0xc, code dwords, u8 0x40,
u8 first user-data dword (bit 7 set for the vertex stage), u8 count of 0x41 records,
then the same 32-byte records from +0x10.

The register block, as far as it is read (offsets in bytes, values little-endian):

| Offset | What | How it was told |
|---|---|---|
| 0x028 | low half 0x11c, high half the block's length - 0x138 | 0x44, 0x4c, 0x54 for the three lengths |
| 0x038, 0x03c | `SPI_SHADER_PGM_RSRC1/2` of the vertex stage | as the compute block's |
| 0x04c, 0x050 | the same of the pixel stage (zero without one) | |
| 0x05c | interpolants the pixel stage takes (1 to 4) | 0x084 is (that - 1) * 2: `SPI_VS_OUT_CONFIG` |
| 0x07c | `CB_COLOR_CONTROL` (0x00cc0010; mode 0 without a pixel stage) | ROP3 0xcc |
| 0x0a0 | `PA_SU_SC_MODE_CNTL` (0x00201a40; bit 1: back faces culled) | |
| 0x0b8 | `DB_DEPTH_CONTROL` (0x007007f0 no depth test, ...f6 depth written always, ...7b3 stencil and less-or-equal) | |
| 0x0bc | `DB_STENCIL_CONTROL` | |
| 0x0c0, 0x0c4 | `DB_STENCILREFMASK` front and back (0x0100ff00) | |
| 0x0d0-0x0ec | `CB_BLEND0..7_CONTROL` (0xe5010504: source alpha, inverse source alpha) | eight alike |
| 0x100-0x11c | `SX_MRT0..7_BLEND_OPT`, which follow from the blend | one value per blend value |

Not found yet in either block: the render target and depth formats, the sample count,
the primitive topology (the words from 0x138 on and the note's first words are the
candidates).

**State.** 0x218 is written (`VICTORY_PIPELINE_PARTS=1`, off by default): the pieces
are put in a container the translator already reads. With it on, over a thousand
pipelines were made in the opening and the picture was black: 1,065 dispatches a frame
dropped, because more than half of those programs do not translate from their pieces.
A container's program record says which root parameter each user-data word is; the
pieces do not, and the translator's `FSPX` reader takes only what the upscaler's
programs need (tables and root descriptors, one word or two each). What is missing,
for both methods: the root parameters from the root signature the description names.
0x1c8 needs, beyond that, the stage programs' inputs and outputs and the host state
from the registers above. Nothing is gained on the picture by either; whether the
title's own way round costs frames (a pipeline asked for again and again) is not
measured.

## The card's memory on an RX 7700 XT (12 GB, Series S profile), 2026-11-26

The card is full two minutes into the world (9.7 to 9.9 GB of a budget of 10.1) and the
PC's memory with it (95 %); the card's own load is 98 %, the processors' 46 %. What
was found, and what was done about it:

- **What the driver gives a buffer of its own**: a multiple of 256 KiB (256 KiB for
  64 KiB asked, 512 for 280), a target 1280x720 about a tenth more than its texels, a
  command list 64 KiB of the card and 1 MiB of the PC, a view heap of a million slots
  30 MB (objects made on a device of their own, the memory in use asked for before and
  after). The copies of title buffers on the card (3,700 from 64 KiB up) and the lists'
  scratch buffers for indirect arguments (3,300) are cut from blocks of 64 MiB now,
  64 KiB each at least (`VICTORY_BUFFER_POOL=0`: allocations of their own).
- **Room kept for targets that never came**: 3,072 MB of the card were kept for render
  targets, the Series X profile's measure; the Series S profile's are 1,350 MB. It is
  1,536 MB there now.
- With both, the textures the title fills that are alive on the card went from 15,800
  (1.4 GB) to 47,000-58,000, and the texture queue no longer finds the card without
  room (152 passes of it did in two minutes).
- **What is neither a block of textures or buffers nor a target**, two minutes into the
  world with 1 GiB of windows in card memory the CPU maps: 3.4 GB. Of it that 1 GiB;
  0.6 GB there before the world (view heaps, the swap chain, what the driver keeps);
  0.47 GB of 39 buffers of more than 1 MiB that are allocations of their own (the big
  copies);   0.25 GB of 2,000 lists and their preludes (64 KiB of the card each); 3,100
  buffers of 4 KiB and less (the hashes kept for copies: 12 MB by their sizes). About
  1 GB is left that nothing here names, and it grows by a few MB in ten seconds. The
  memory line counts the buffers that are allocations of their own by size.
- **Those 3,100 small buffers were the missing gigabyte.** The line counted what was
  asked for, not what the card gives: 1000 buffers of 64 KiB made one after another
  took 250 MB of the card's memory, 256 KiB each (an ignored test,
  `host::place::tests::small_buffers_cost`). The 3,100 buffers of 4 KiB and less
  therefore held about 775 MB, not 12, and the hashes kept for each copy (16 bytes a
  piece) cost more than the copies they watch. What is written before it is read —
  the hashes of a copy (filled whole when made), each list's indirect budget (zeroed
  by its recording), the bytes between copies — is cut from the blocks of buffers now
  (`Host::scratch_uav`), 64 KiB of a shared block instead of 256 KiB of its own. The
  line counts what the driver gives, 256 KiB apiece at least. Two and a half minutes
  into the world, Series S profile, the same settings: allocations of their own
  39 buffers 0.47 GB + 3,100 of 4 KiB and less -> 20 buffers 0.41 GB and none small;
  the textures alive on the card 38,000 -> 42,900, the frame 21-23 -> 24.1 fps
  (41.5 ms), the card 9,680 MB of 9,941 in use. (One run each; the scene is not
  exactly the same.)
- **Empty blocks of textures** (nine of 64 MiB when the card is full) are kept on
  purpose: memory given back and read by a list still running lost the device twice.
- **What the card reads over the bus**: the copies of windows of title memory (the
  title's own vertices, indices and constants, 3.4 GB) are in the PC's memory, because
  the card has no room (`VICTORY_CARD_WINDOWS_MB`, 0). Moving the title buffers from
  64 KiB up to the card took the card's working time from 45 to 32 ms a frame.

How a frame was measured to go (four frames of 43 ms in the world, `VICTORY_TIMELINE`,
and `VICTORY_GPU_TIMING`): the card running submissions is the longest link of the
chain (12.7 ms a frame, one frame in four all of it); the title's own commands take
19 ms of the card and more (dispatches 7.9, indirect executions 5.4, mesh draws 3.3);
the submitting thread works 32 to 35 ms. The lines the frame meter logged once a second
took 16.8 ms of the frame they fell in; they are written by a thread of their own now.

## 30 frames a second on an RX 7700 XT, and what took it there (2026-11-26)

Series S profile, i5-14600KF, 32 GB, the opening cutscene; one run a figure unless said.

| Step | fps | The preparing thread, ms a frame |
|---|---|---|
| Before | 25 | 38.9 |
| The title's heap of constants in place (`VICTORY_PORT_CONSTANTS=1`, avatarmanz's renderer port 1; now in the profile) | 27-28 | 35.1 |
| The heaps' records asked for when the title submits (`VICTORY_HEAPS_AT_SUBMIT`) | 28-29 | 32.5-33.5 |
| Blocks of the title's general allocator write-watched (`VICTORY_ALLOC_WATCHED`) | 29-30 | 30.3-32.0 |

**The title submits two or three times a frame; the layer makes 28 parts of it.** Its lists
wait for memory values (labels the card writes when a list has run: 19.7 of some 20 waits
a frame are met by a list sent before), and each part was prepared on its own: the write
watch asked, the heaps' records brought up, the refreshes flushed. 13 parts are joined to
the one before; what stands between the others, a frame: a wait for a memory value 5, a
step done on the CPU 3, the most lists joined 2, memory the parts before write 1 (the log's
"parts not prepared with the one before them").

**The heaps' records.** The preparing thread asked for a round of them per part, when it
came to the part, a frame of work after the title wrote the records, and stood 6 to 7.5 ms
a frame until the round was over. Asked for when the title gives its lists to a queue, one
round for all their parts: 2 ms left. It rests on the waits being the card's own labels:
were one released by the title's CPU after it wrote more records, the round would be too
early.

**A 20 MB buffer copied whole twice a frame.** `XMemAllocDefault` took its big blocks from
the process heap, where nothing says what was written; the title places buffers of the
card's in them (seven at the start of the world). The copy of one, written by shaders
only, was copied whole at every refresh (42 MB, 4.6 to 6.3 ms a frame), sent up whole and
fetched back whole. The blocks are allocations of their own now, write-watched. The log
says so when a copy of a megabyte and more is neither watched nor guarded.

**In place is not the answer for what shaders write.** The same blocks opened as the
card's own memory (`VICTORY_ALLOC_INPLACE=1`): 21 fps, the card waited for 13 to 15 ms a
frame. A heap opened on the PC's memory is read and written over the bus. Renderer port 1
gains because constants are small reads; geometry and shader-written buffers belong in the
card's memory. What is left for those is not to fetch back what nothing on the CPU reads
(69 MB a frame fetched, 21 of them this buffer): it needs the pages kept from the CPU until
something touches them.

**The 30 is ours, not the title's.** The title asks for a frame every 33,333 us and is
paced by its wait for our clock. `VICTORY_FRAME_US=16667`: the menus run at 60 fps; the
world stays at 29 to 30, the preparing thread's 31 ms. Whether the title's own timing
holds at 60 has to be looked at by a person.

**What the title's main thread does while it "runs".** 60 % of its time is a loop
(`monitorx`, `mwaitx`, `Sleep(0)`, image +0x4fb0bd0) on one of those labels: it waits for
the card, so its 15 ms of a frame's chain are the preparing thread's, the card's and the
landing's, seen from the other side. 11 of 20 processors are busy: the title's threads 3.6,
the eight that translate lists 3.3.

Measured and no different here (on and off by turns of 8 s in one run): avatarmanz's
`VICTORY_ASK_SPREAD`, `VICTORY_UP_QUEUE`, `VICTORY_JOIN_BARRIERS`; worse:
`VICTORY_LANE_CORE` (48.6 ms a frame against 37.4), `VICTORY_ASK_NORESET` (40.2 against
37.8); `VICTORY_PORT_DESCRIPTORS` with port 1: 25 fps, and some 20 slots a second its
patches do not tell of (writers at title +0x18849c, +0x4e9b4e2, +0x2474afc and a system
copy). The list threads on the slow cores: 28 fps both ways. More geometry in card memory
(`VICTORY_WINDOWS_BAR_MB=2048`): 25 fps both, 22,100 textures alive against 32,700.

codec-d/cool_fox (avatarmanz at 0a43566 with six commits for an RTX 5090: two planes
composed, a guard on the PC's memory, XMA corrections, NVIDIA lowering), built and run
here with its launcher's settings: 13 fps in the same scene, then 5 (the card waited for
50, then 176 ms a frame: 11.8 GB of textures uploaded on a 12 GB card).

## Past 30: the wall is the card now (2026-11-26, later)

With the clock at 60 Hz (`VICTORY_FRAME_US=16667`) the opening cutscene runs at 30 to 31
fps, and what a frame waits for has changed. A timeline of 40 frames (35.1 ms each): the
card running submissions 11.5 ms of the chain, the thread that prepares them 4.9, the
title's own 3.6, the heaps' records 2.4, the landing 1.4. The card's own stamps
(`VICTORY_CARD_SPANS=1`): working 29.0 ms a frame, idle 3.8: busy 88 % of the frame.

**What the card's time is** (`VICTORY_GPU_TIMING=1`, each command timed alone, so the sum
is over what a frame takes): 34.4 ms in 8,700 commands a frame: dispatches 15.4 ms in
3,111, indirect commands 13.6 ms in 4,730 (compute and mesh), mesh draws 2.2 in 350, the
alias copies 1.4 in 77, draws 1.3 in 290. The forty heaviest are 18 ms, compute nearly all
of it: `indirect [compute 919bafe017d59512]` 2.67 ms in one call (253 blocks, 69 handles
made from the heap, 35 samples, 61 raw buffer loads), `dispatch [compute
39f0e9aeb156a61c]` 2.39, `7a071f906f98311b` 1.08, `34fb51b8a0312e82` 0.85. Drawing the
scene's 4,400 meshes is under 4 ms. 720p for 29 ms is some 31 ns a pixel; a PC game of this
kind on this card takes a fifth of that.

**The title is told the card does nothing.** It asks for four timestamps a frame in pairs
and all are written when the frame's last submissions land. `VICTORY_GPU_LOAD_MS=30` (the
pairs 30 ms apart): the card's work the same 29.6 ms, so it does not size its picture by
them here.

**The heap.** Our own code asks the heap for 29,500 blocks a frame and makes 7,200 over at
another size: two million calls a second on the one heap of the process, 11.7 % of the
time the preparing thread runs (profile p2), 8 % of the landing's. Blocks of a megabyte and
less are now kept by the thread that lets them go for its next ones (`crate::heap`,
`VICTORY_HEAP=system`: as before): 27,500 of 36,400 a frame answered without the system;
that thread 30.1 ms a frame against 30.9. (mimalloc was the first thought: not fetched
without the owner's word.)

**Who touches the blocks only shaders write** (`VICTORY_ALLOC_KEPT=1`: the blocks of the
title's general allocator are kept from the CPU until something touches them). Of seven,
two are written at once by the title, four (8, 16, 16, 8 MB) by nothing in 170 s, and the
20 MB one is first read 37 s after it was made by the landing that fetches it back: 21 MB
a frame brought down for nobody. (We wrote zeros over each of them when it was made, over
the system's own zeros: not any more.)

## What comes back from the card comes back for nobody (2026-11-26, night)

The largest single thing found that day. What shaders write to a copy in the card's memory
was found by a compare on the card after every submission (8,300 pieces of 64 KiB a frame:
544 MB looked through, in 500 passes of ours), fetched (69 MB a frame), merged into title
memory, and found there as written by the next refresh, which tells it from the title's
own writes and sends some of it up again (17 MB a frame up, 14 of them that).

| Clock at 60 Hz, opening cutscene | fps | Preparing thread | Card working | Fetched a frame | Picture |
|---|---|---|---|---|---|
| As it was | 30-31 | 30.3 ms | 29.0 ms | 69 MB | right |
| Nothing compared or fetched (`VICTORY_CARD_NO_BACK=1`) | 34-35 | 21.3 | 27.3 | 0 | black |
| Not for copies of 1 MiB and more (`=1024`) | 35 | 25.5 | 27.1 | 9 MB | right |
| The card owns copies of 1 MiB and more in private memory (`VICTORY_CARD_OWNED=1`) | 31-32 | 25.6-26.5 | 28.8 | 35.5 MB | right |

- **The title does not steer by it.** With nothing fetched its calls are the same: 4,500
  draws, 38,500 list calls, 12,200 buffer copies a frame. The black picture is a few small
  things read back (under a megabyte in all: the 9 MB of small copies hold them).
- **Copies over the same memory are not why either**: 1,412 pairs among 4,475 copies, 54 MB
  where shaders write one of the two (`VICTORY_OVERLAPS=1`).
- **So the bytes have an owner** (`crate::host::owner`). A copy shaders are declared to
  write for the first time is brought up to date whole with its pages read-only, then its
  pages are kept from the CPU (`PAGE_NOACCESS`) and nothing of it is compared, fetched or
  refreshed. The first thing on the CPU that touches it faults; the handler has the copy
  fetched by a list of its own on the drawing queue, and the copy is as every other from
  then on. Exact whoever reads: nobody has to be known ahead.
- **Who opens the copies** (the log names the instruction and its callers): writes of the
  title's (title +0x5384ff5, +0x50e23ed: buffers it fills beside what shaders write there),
  and refreshes of other host copies over the same title memory (`Shadow::copy` from a
  flush: a window a draw reads vertices through, where a shader wrote them through the
  buffer's copy). The second kind is the card talking to itself by way of the PC.

**The first part that is played is heavier than the cutscene.** One run (z1, the clock at
30 Hz, the defaults) left the cutscene after 65 s and stood in the cell that follows: 5,800
to 6,000 draws a frame against 4,400 to 4,600, 23 fps, the card waited for 8 to 10 ms a
frame, the preparing thread 27 to 29 ms. Every figure above is the cutscene's; the walls
are the same ones, the card's first.

What is left of the 69 MB, and what each needs:

1. Copies in views of the title's page pool (0xac6160000+0x3200000 alone 10 MB a frame, and
   most of 0xae.., 0xaf..): the guards there set and lift protections of their own; the
   owner's pages have to be theirs to leave alone.
2. Buffers the title writes in part (0xac6160000: 164 pieces sent up, 112 others fetched;
   0x963410000): an owner by piece of 64 KiB, not by copy.
3. A reader that is another host copy: the bytes from the card's copy by a copy on the
   card (as `texture::alias` does for a view in another format), or one host buffer for both.
4. The pages opened before the bytes are in them: memory of ours for the title (the
   allocator's blocks) can be a section with a view of our own to fill it through.

## What the translated shaders are made of (2026-11-26)

The title's own containers are written down as it uses them:
`VICTORY_SHADER_DUMP=<folder>` (a diagnostics build; the folder outside the
repository) writes each one once as `<blake3>.cdxil`, the name a command's
note starts with (`[compute <16 hex>]`). `victory translate <folder>` with
`VICTORY_SHADER_CENSUS=<file>` then writes a line per emitted program: the
console program's instructions by class and the host module's by kind
(`emit::census::KINDS`). Two minutes of the opening dumped 19,127 containers,
of which 721 translate.

Those 721 are 1,289,934 console instructions and 3,148,778 host ones (2.44
times). Of the host ones 382,415 are bitcasts, 200,883 extractions from a
load's four components, 128,444 phis and 101,594 the blocks they live in —
the driver folds the first two away — and 1,432,841 arithmetic operations
for 915,094 console VALU instructions (1.56 each). The four biggest compute
programs are 19,846 to 19,458 console instructions in 1,632 blocks each: the
title's uber-shaders, most of their code cold on any one lane.

**The largest single cost was the legacy product.** `v_mul_legacy_f32` is what
the console's compiler writes for nearly every multiply (2,270 of the 19,846
instructions of the biggest compute program). The console gives 0 where one
factor is ±0, whatever the other is; no host instruction does, so the
translator asks whether the smaller magnitude is zero (`fmin(|a|, |b|) == 0`)
and selects a zero — three instructions where the console has one, on one in
eleven of its instructions. Written as the plain product
(`VICTORY_LEGACY_MUL=0`), the same 721 programs come to 2,567,789 host
instructions, **18.5 % fewer**: compute 1,331,549 → 1,076,607 (19.1 %), pixel
20.4 %, mesh 14.1 %, vertex 11.7 %; the biggest compute programs 17 to 21 %
fewer, and half the selects (377,931 → 186,027) go with it. What differs: a
product of a zero and an infinity or a NaN is that instead of a zero; every
other pair of values gives the same bits (a zero's sign may differ). Off by
default; part of the translation, so the shader cache keys on it.

(Method note: `victory translate` without `--pipelines` makes no device, so
`host_rdna()` is false and the census counts the forms spelled out for every
host. A run takes the RDNA ones, where a legacy product is the `fmin`, a
compare and a select; the three instructions are the same either way.)

## What is next: the three walls to 60 and beyond

In the order the measurements put them (an RX 7700 XT; each has to come down by about
half for 60 frames a second, and all three stand at about 30 ms now).

1. **The card, 29 ms a frame.** Four fifths of it is the title's compute. To look at, the
   heaviest first: what the forty costliest programs do in their host form against the
   console's (handles made from the heap inside loops, loads of four dwords for one, the
   wave's first lane read for what is one value), and what they read from the PC's memory
   over the bus (renderer port 1 puts the constants there; a program run a million times
   a frame reads them each time). Then the 3,000 small dispatches (skinning, culling),
   7 ms: what stands between two of them on the host that does not on the console (689
   waits for every earlier shader write a frame). The alias copies, 1.4 ms: one texture
   with two views needs formats the host may cast between (R32_UINT and R9G9B9E5 are not
   of one family; relaxed casting at creation, `CreateCommittedResource3`).
2. **The thread that prepares submissions, 30 ms a frame.** One preparation for all the
   parts of one call of the title's, begun when it calls and beside the thread that sends
   (the heaps' records are asked for that way already): the write watch asked once a call
   (4.7 ms in 15 askings now), the refreshes flushed once (8.3 ms in 30 flushes, 9,400 of
   10,600 spans copying nothing).
3. **What goes back to the PC's memory, 69 MB a frame**: 9 ms of the preparing thread,
   1.7 of the card's, all of the landing's. Memory with an owner does half of it exactly
   (`VICTORY_CARD_OWNED=1`, above); the four things listed there are the other half, and
   then it can be the default.

Smaller: outdoor flicker (one indirect record that uses up its list's budget:
`VICTORY_INDIRECT_REST=0`); stalls while pipelines are made (`victory warm`); the PC's
memory, 24 to 25 GB private with 2 to 2.6 GB free.
1. The preparing thread: split what one submission does over two threads in a row; ask
   for the next submission while this one is sent; view the card's own buffers where fills
   copy them now.
2. The card: small buffers' copies in its own memory (`VICTORY_CARD_BUFFERS_MIN_KB=64`,
   after fxceless c36971b2, wins over `VICTORY_CARD_BUFFERS_MIN_MB`; not measured here
   yet), and the passes that run for nothing
   without rays.
3. Stalls: 270-660 s of a session were full of frames over 200 ms, almost all while new
   pipelines were made. `victory warm`, and a quarter of the workers for the slow ones.
4. Outdoor flicker: suspected to be one bad indirect record that uses up its list's
   budget, so that every later draw of it is empty. The log's frame line counts them;
   `VICTORY_INDIRECT_REST=0` is the experiment.
5. Memory, 40-46 GB private: lists' allocators 9 GB, windows 7, uploads 4
   (`VICTORY_MEMORY_MAP=1` lists it).

## The card's 26 ms taken apart with a replayed frame (2026-11-26, afternoon)

An RX 7700 XT, the Series S profile, the opening cutscene with the clock at 60 Hz: 34.5
to 35 frames a second, the card working 25.7 ms a frame, the preparing thread 26 to 27
(renderer ports 6 and 7 of avatarmanz/VIctory-7900XTX, on in the profile: 28.1 before).

`victory replay` lost the card on every capture of this tree: the host makes its new
memory not resident and then resident (`host::not_resident`), the capture records the
flag, and the replay made every resource with it and nothing resident, so its first copy
into a texture was a page fault. Fixed; a failed load now says the stage, the batch,
what the debug layer said and where the system's breadcrumbs stopped. A captured frame
(40 submissions, 291 lists, 39,000 host commands) runs in 26.3 ms of the card with no
title beside it, against 25.7 to 26 in the title: the frame is the card's.

By kind (`--timing detailed`; a command's time is a share of its list): the title's
3,063 dispatches 9.1 ms, its 3,407 indirect mesh draws 3.5, 920 indirect dispatches 3.2,
733 draws 2.4, 1,884 barriers 1.1, our 159 argument clamps 1.0, 346 indirect draws 0.9,
242 texture copies on the card 0.7, our 520 compare passes 0.6, 150 buffer copies from
the PC 0.55. Of the 3,742 dispatches 734 are ours. What the experiments say, each
against 26.3 ms:

| replay | ms | what it is |
|---|---|---|
| `--no-barriers` | 24.2 | the 870 waits for every earlier shader write (690 for the title's barrier groups, two for each clamp); in the title, `VICTORY_BARRIER_TEST=both`: 24.8 against 25.9 |
| `VICTORY_REPLAY_HEAPS=<the heap in place>` | 24.6 | the title's 185 MB of constants and skinned vertices in the card's memory instead of read in place over the bus (renderer port 1) |
| `--skip-hash-passes` | 25.2 | our compare and hash passes |
| `--buffers vram --only-buffers "name=list scratch"` | 25.6 | lists' scratch memory in the card's; 25.2 against 26.4 with the tables' constant buffers copied there (`VICTORY_PORT_CBV_PLACE=0`) |
| all four | 22.3 | |
| `--buffers vram` (everything) | 163 | 14.8 GB asked of a 10.3 GB budget: what the card does when its memory is over |

So four fifths of the card's time is the title's own work, about 21 to 22 ms here, and
ours is about 4. In the title, lists' scratch chunks in card memory the CPU maps
(`VICTORY_SCRATCH_CARD=1`) gave no gain that could be told from the noise; it stays a
switch. The title's memory use stands over the card's budget all the time (10.9 GB in
use against 10.25).

### Copies of one memory

Every 64 KiB block of view memory that shaders are declared to write is shown by the
title at two addresses or more (two to sixteen), and two blocks in three have a copy at
more than one of them: one buffer of 24 or 50 MB over all of a pool's pages, and small
buffers over parts of the same pages. Each copy has its own bytes on the card, and they
meet only by way of title memory: compared, fetched, put into title memory, found
written by the guards of the other view, sent up again. That is what the compare passes
and most of the bus traffic are for now (4,000 to 5,400 pieces compared and 24 to 35 MB
fetched a frame).

Memory with an owner took such blocks through one copy since the morning. That was not
exact: what shaders then wrote through the other copy stayed in that copy's bytes (620
blocks a frame declared written that way, run k3), and the refresh of the other copy
brought the block down again (1,350 of 3,000 taken). Now a block or a copy is taken only
where no other copy holds the same memory (`shadow::copies_holding`): 1,390 blocks
taken, 75 brought down, 33 to 34 frames a second against 34.5 to 35
(`VICTORY_CARD_SHARED=1`: as before, for measuring).

What follows from it, and is the next piece of work: the copies of one page share the
card's memory. Card sides as reserved buffers whose 64 KiB tiles come from a pool
(`tiles.rs` does it for textures); when a block is taken for the card, the other
copies' tiles of that page are mapped to the taker's (`UpdateTileMappings`, in queue
order), so that a shader's write through any of them is the others' at once, as on the
console; when the block is brought down, they get their own tiles and the bytes again.
Then every block shaders write can be the card's alone, and nothing of it is compared,
fetched or sent up.

Done the same day (`crates/victory-gpu/src/host/pages.rs`, `VICTORY_CARD_PAGES=1`, on in
the Series S profile). A title buffer's copy that begins on a 64 KiB line has a reserved
buffer for its card side, every page a tile of its own from a pool of 64 MiB heaps (139
copies, 326 MB, 2 ms of `UpdateTileMappings` in all). When the owner takes a block that
other copies hold, each of them that shaders write must be able to show the taker's tile
in that page, and does from the lists sent after it on; a holder with a write of a
submission in the air there puts the taking off until it has landed; a holder no shader
writes (a window outside buffers) is left as it is, since its read of the block brings
it down as any read does, and the first write declared for such a copy brings down what
it does not share (`owner::written_first`). Brought down, the others show their own
tiles again and send the page up whole. Opening cutscene, the clock at 60 Hz:

| | fps | compared a frame | fetched | sent up | blocks taken / brought down |
|---|---|---|---|---|---|
| blocks with other copies not taken (exact) | 33-34 | 5,400 pieces | 35 MB | 18 MB | 1,390 / 75 |
| taken through one copy (not exact, `VICTORY_CARD_SHARED=1`) | 34.5-35 | 4,100 | 25 MB | 8.8 MB | 3,010 / 1,210 |
| the copies share the tile (exact) | 35.3-36.3 | 1,050 | 11.6 MB | 3.3 MB | 2,940 / 220 |

The log counts the blocks shaders are declared to write through a copy that neither has
them nor shares them: 620 a frame in the second row, none in the third. What is still
compared is private memory (the title's general allocator) under copies that begin off
the 64 KiB lines.

### What did not pay (the same day)

- The waits for every earlier shader write. The title's barrier groups read as the
  host's own structures (`VICTORY_BARRIER_CENSUS=1`, taken where the title calls: lists
  translated later keep only their count): of 20,000 calls 8,500 hold buffer barriers
  alone, 2,100 textures alone with no unordered access before, 1,300 a global one; of
  the barriers 17,300 are a buffer's unordered access to unordered access, 8,200 a
  buffer's unordered access to shader resource, 5,000 a texture's unordered access to
  itself. So nearly all of them are waits the console makes too; one call in ten could
  go. Joined where nothing stands between two (`victory replay --merge-barriers`): 25.6
  ms of the card against 25.8.
- Constants in the card's memory. With the tables' constant buffers copied into lists'
  scratch in card memory (`VICTORY_PORT_CBV_PLACE=0 VICTORY_SCRATCH_CARD=1`) the card
  works 24.9 ms a frame against 25.0 and the preparing thread 28.5 against 26: the
  replay's 1.2 ms do not show in the title. Without renderer port 1
  (`VICTORY_PORT_CONSTANTS=0`) this profile now stops where the world begins (the title
  stands still for half a minute, then its own crash reporter runs): not looked into.
- A longer rest for blocks the title writes every frame: `VICTORY_GUARD_HOT=40` the
  same frame rate, `VICTORY_GUARD_REST=30000` 33 fps against 35.
- Ownership from 64 KiB up (`VICTORY_CARD_OWNED_KB=64`): 780 pieces compared a frame
  against 1,000, the frame rate the same.

## The picture: textures and the upscaler (2026-11-26, evening)

What was seen on the screen: surfaces without their textures or with coarse ones, and a
dotted, pixelated picture (hair, the edges of shadows, nets).

**Textures had no room.** The card stood over its budget (10.3 GB in use of 10.2), the
pass that makes textures picked by index found no room 1,700 times in three minutes, and
57,000 host copies of textures were let go (5.5 GB) for 21,700 alive. A gigabyte of the
card was the windows of title memory mapped by the CPU (`VICTORY_WINDOWS_BAR_MB=1024`),
which no longer buy frames: 34.4-35.3 fps without them against 34.3-35.8, the card
working 27.0 ms a frame against 25.0. Without: 29,000 copies alive, 26 passes without
room, 9.9 GB in use. The profile has them off now.

Of the copies alive, 1.0 to 1.7 GB are levels whose title memory is not mapped (the
largest level of 7,000 textures): the title views a texture from a level it has not
streamed in yet, and the copy is made from the first level viewed. To do: from the
first level held, grown when the larger one is mapped (`resource::want_level` has the
replacing). `VICTORY_LEVELS_ON_DEMAND=1` is not that in this tree: it turns the levels
viewed alone off and makes no texture level by level (12.5 GB asked of the card, 5
fps).

**The texture store** (`VICTORY_TEXTURE_STORE=<folder>`, `scripts\play.ps1
-TextureStore`): the larger levels of textures seen whole, kept on disk and given at
once when the texture is next viewed from a smaller level. A first run of three
minutes kept 2,700 textures (1.5 GB); in the next, 2,770 to 4,100 host copies were made
whole from it where the title would have streamed the levels in over seconds (a slot
written while its memory is not there waits 6 to 7.6 s on average to be shown).
`VICTORY_TEXTURE_STORE_KEEP_MB` bounds what of it stays in memory (4 GiB unless given).

**The dots are the upscaler's.** On Series S the title picks FidelityFX, and its FSR 4
passes come as pipelines made for the console's card: `DeserializeComputePipelineStateX`
is not implemented here ("FAILED hr=0x80004001 for 'FFX_FSR4_PASS_0' -- falling back
to NewBE", 29 passes), `amd_fidelityfx_upscaler_xbox.dll` is refused by the host
policy, and what the fallback gives leaves everything the title dithers for the
upscaler to smooth as dots. With the title's own upscaler (`VICTORY_UPSCALER=6`, what
it uses on Series X; the same 1280x720 to 1920x1080) the picture is smooth, at the
same frame rate. The profile uses it now. Its scale (`VICTORY_RENDER_SCALE=0.8`) ends
the title at its start on this profile, as the other sizes do.

## The PC's memory with the title's ray tracing (2026-11-26, evening)

An RX 7700 XT, 32 GB, the Series S profile with `-RayTracingLow`,
`VICTORY_QUIET_GUARD=1` and `VICTORY_PRELUDE_POOL=1`; the PC's other programs hold 12
GB (a browser, the editor, chat). The title then stands at 26 to 27 GB private with
0.9 to 2.7 GB of the PC's memory free, and a run in which it fell to 0.9 GB had frames
of 380 and 625 ms and 18.6 frames a second on average (run b0) where the same build
gives 29 with 2.7 GB free: the average is the paging's before it is the card's.

Where it is (run ram-map1, three minutes in; committed, and of it in memory):

| | MB | in memory |
|---|---|---|
| the title's heaps (private) | 4,269 | 2,280 |
| views of the title's pages | 5,862 | 2,590 |
| the process heap | 2,406 | 1,691 |
| windows of title memory, 4 MiB each | 2,744 | 2,127 |
| other private allocations of 1 MB and more | 4,249 | 3,155 |
| write-combined | 12,248 | 2,952 |
| files and mappings at host addresses | 7,728 | 2,088 |

- The 125 write-combined allocations of 64 MB (8 GB) are the card's own heaps as the
  driver maps them (texture blocks, buffer blocks, the arenas' card sides): they are
  not the PC's memory. What is: the memory shared with the card, 8.2 GB in use, of
  which the windows outside buffers 3.3 GB (794 of them), the copies of title buffers
  1.9 GB and the CPU's sides of the arenas of memory by address 1.3 GB (20 private
  allocations of 64 MB).
- The mappings at host addresses are views of ours of the title's sections (the
  guards', renderer port 1's): the same pages as the title's views, counted twice.
- Of the windows, 1,671 MB lie between the lowest and highest address ever asked for
  in each. By what could be opened in place (`VICTORY_INPLACE_PROBE=1`): 67 (316 MB)
  in private memory committed whole, 426 (1,740 MB) in view memory with unmapped pages,
  288 (1,168 MB) in private memory not committed whole or guarded, none in one run of a
  section; the title's four sections are made reserved, which the host refuses as
  heaps.

`VICTORY_MEMORY_DIRECT=1` runs in the title (run ram-direct1, no error): the title's
heaps 3,029 MB against 4,269, 25,150 MB private against 26,000 to 26,290 at the same
time of the run. With `VICTORY_WINDOW_KB=1024` and `VICTORY_WINDOW_TAILS=1` the title
stayed at its first menu (11 draws a frame for 100 s, run ram-fine1): not looked into.

## Ray tracing in play, and the merge of Belobug's branch (2026-11-26, night)

An RX 7700 XT, 32 GB, the Series S profile, `scripts\play.ps1 -RayTracingLow`, the clock
at 60 Hz. What is asked for is 40 frames a second in the mean with the title's ray
tracing and every texture; where it stands is 32 to 34 in the first scene that is
played, and what follows says what the eight are made of.

**The scenes.** A run's first two minutes are the opening cutscene (1,000 to 4,900
draws a frame by shot, 28 to 31 fps); holding A skips it (`VICTORY_KEYS=..,58-63:Enter`
in a measuring run) into the first scene that is played, the window (4,400 to 4,900
draws, the same every frame: what the figures below are of), and the dormitory behind
it is heavier than both (5,000 to 6,300 draws, the card working 41 to 47 ms a frame, 21
to 23 fps). A pad held by a person does the same: two runs (m0, s50) left the cutscene
that way and were first read as slower builds.

**Belobug/VIctory-Belobug-AMD `a-publier`, merged** (three commits over upstream
c9c932c, which was here already):

- e942896: descriptor slots of a list let go when its submission lands (not 64
  submissions later), the indirect batches' buffers from a pool shared by the lists,
  views of a host copy that starts at a later level clamped for arrays too, textures
  capped by the card's room (`VICTORY_TEXTURE_MAX`: 1024, 512 from 92 % of the budget,
  256 from 97 %), an idle texture let go sooner, a guard for the AMD compiler crashing
  on an RX 9070 XT, the panel in release builds.
- 5d6bd84: a title texture's upload is recorded in the submission's prelude, after the
  refresh that finds its memory changed, not by the next list that samples it: no frame
  late, no flicker where parts of a frame are joined.
- cd8dfc0: the title's FidelityFX served by AMD's PC library (`-Fsr4K`, Series X at 4K
  only, the DLL the user's own): not what this card and profile can use yet.

Four conflicts (the prelude now takes the pooled list and the uploads; the batches'
buffers are still cut from the buffer blocks). One thing changed: the cap's percent
counts the card's memory less what the texture blocks hold free, as the texture queue
and the sweep do. Counted whole this card stands over 97 % for good and every texture
made from then on would have been one of 256. The window scene after the merge: medians
of 29.2 to 30.3 ms (33 to 34 fps, run g1) against 33.5 ms before it (run rt-low2); the
picture whole in the shots.

**What sets the frame there** (run g1): the card works 28.2 ms and is idle 2.0; the
thread that prepares submissions works 25 ms. Each tried against that:

| | frame, ms (median) | the card works | the preparing thread | |
|---|---|---|---|---|
| as it is | 29.2-30.3 | 28.2 | 24.1-26.0 | 28,700 textures alive |
| trace programs one frame in 12, not 6 (`VICTORY_TRACE_EVERY_ALT=12`, by turns) | 29.5 against 30.0 | | | |
| windows of title memory in card memory (`VICTORY_WINDOWS_BAR_MB=1024`) | 29.0-30.4 | 27.8 | 24.8-25.6 | 18,400 alive, 198 passes without room against 54 |
| render scale 0.5 (`VICTORY_RENDER_SCALE=0.5`: 960x540) | 28.7-29.4 | 24.9, idle 4.9 | 26.0-26.7; the title waits 5.3 ms at presents | |
| the same, built with thin LTO and one codegen unit | 28.9-30.2 | 25.3 | 25.7-26.6 | |
| the title's own settings file lightened (`lodScale` 0.5, `rtDynamicShadows` false, `motionBlur` false, `uiResolution` 1920x1080 and others) | 30.4-32.2 | 29-30 | | the same draws, the same 13 trace programs, the 3840x2160 viewport still there: not read, or not at start; the file put back |

So a smaller render size is worth 3.3 ms of the card and not one frame: below 2/3 the
frame waits for the preparing thread, at 26 ms. Forty frames a second there is the card
under 24 ms (the render scale does it, for a softer picture) **and** that thread under
23, and its parts are what they were in the afternoon: 4.2 ms asking the write watch,
13 to 14 in the lists' steps (8 of them 10,700 refreshes of which 9,300 copy nothing),
2.3 for copies in the card's memory, 2 to 3 waiting for the heaps' records. Link-time
optimisation does not touch it, so it is not our instructions that it spends. In the
dormitory the card alone sets the frame, and there the render scale is worth what it is
worth of the card (39 to 43 ms against 41 to 47).

**The PC's memory.** Two runs in three before these were spoiled by it (0.9 to 1.9 GB
free, lines of 2 to 3 fps while the system paged). With ray tracing the profile now has
`VICTORY_QUIET_GUARD=1` and `VICTORY_MEMORY_DIRECT=1`, and on Series S
`VICTORY_PRELUDE_POOL=1`: 3.4 to 4.4 GB free in the window scene against 2.4 to 2.8,
24.3 to 24.6 GB private against 26.1 to 26.8, and the preparing thread 23.3 to 24.5 ms a
frame against 24.1 to 26.0 (run v1, the profile as the menu starts it: 32 to 33 fps, no
line of the log an error, the picture whole). A memory map of the afternoon's kind (run
s50, 20 GB in memory): buffers shared with the card 3.3 GB (12.4 GB of addresses),
other private allocations of a megabyte and more 3.9, files and mappings 2.9, views of
the title's pages 2.6, the process heap 2.4, windows of title memory 2.3, the title's
heaps 2.3.

**The card's memory.** While the world loads 40,000 textures are uploaded (4.2 GB in
5.1 GB of blocks), a third of them let go in the next twenty seconds, and the blocks
stay: 4.4 to 4.9 GB of them for 2.7 to 3.1 GB held, since a block goes only when it is
empty and each has textures that live long among those that do not. Found on the way:
the blocks made ahead while the card had room were never let go once it had none (the
pool only remembered what it was told to keep when it let blocks go: 7 empty blocks,
448 MB, with the card at 99 % of its budget, run p1); they go now. What would bring the
blocks to what they hold: moving the copies out of the emptiest block the way
`resource::want_level` replaces a copy (the slots show the old one until the new one's
upload has run), with `descriptor::host::levels_changed` to have the slots made again.
Not done.

**Two games at once.** A second session ran its own game and builds beside these runs
and spoiled four of them (a run's last tables read 109 and 205 ms for the card, the
system's commit 702 MB from its limit). The measuring script now says how many seconds
of a run another game or a build ran, and the least memory the PC had free.

## The other repositories, compared

Read up to 2026-11-25: Marvisak/VIctory, fxceless/victory-fork, DexNom/VIctory,
Belobug/VIctory-Belobug-AMD, FQY55kai/ZJOU-VICTORY. What was taken is in the history,
each commit naming its source. In short:

- Marvisak's commits hardly apply any more; nearly all of what they fix is here, often
  another way. Taken: nearest normalized codes, d16 views, shader model flags.
- fxceless: the base. Newest taken: texture read-back, small buffers on the card, hashes
  over a buffer's own bytes.
- DexNom: thread counts of ray dispatches, cube gradients, joined sparse ranges.
- Belobug: pipeline workers for screen passes, the ambient stand-in's idea, pipeline and
  constant buffer bookkeeping (romainhedouin).
- ZJOU: render targets not set again; some shader corrections are being looked at.

### Read again on 2026-11-26 (evening): every branch of every clone

Fetched and read by commit message and chosen diffs, nothing of it built or run here
unless said. "Not here" was checked by searching this tree for the switch or the
function named.

**Taken that day.** Belobug `a-publier` whole (cd8dfc0: the PC FidelityFX bridge for
the Series X profile, textures uploaded in the submission's prelude, the texture cap by
the card's memory `VICTORY_TEXTURE_MAX`, indirect batch buffers from a pool, the guard
on the driver's compiler; merge 318d2f6). avatarmanz 5a2f8c6 (`VICTORY_PIPELINE_LOW_MEMORY`,
off). fxceless 41f9857 (a moved view keeps the title's minimum level) and af23c65
(`submit_split` gives its tables' slots back). The last three applied as they were.

**Not taken yet, by what they would help.**

- The card's memory and the textures: xcl b6fd1f27 (branch `amd-dev`: render targets
  the title maps at two addresses over the same pages share their card memory, 3,597 to
  1,738 MB of targets there; here targets are kept by address, 722 of 1,388 MB);
  Marvisak 9a12a45 (depth twins share their scratch buffers, one a size); fxceless
  e4245f3, the part for its issue 3198 (a texture the card had no room for is asked
  again after a second: here `refused` is for the run); Marvisak 61bfc77 (placed
  unordered texture views as real ones: this tree's log says "placed unordered texture
  view not mirrored", 11 times in two minutes); codec-d/cool_fox f4b48ad (one-shot bakes
  wait for their textures, with `VICTORY_UPLOAD_AHEAD` and `VICTORY_SHOW_AFTER` as the
  profile has them); fxceless 19a45d1 (a view of a texture the card wrote is copied
  from it after an upload too).
- The preparing thread: xcl 71b8e6e0 (a submission whose lists were all closed before
  the last look at the write watch began takes none of its own, a quarter of them
  there); DexNom bc3cd4b (only the root views a program stores through are declared
  written; the translation cache's form changes).
- The translator, to be done as one lot since each change makes the cache again:
  Marvisak cc7ec2a (64-bit compares AMD's driver gets wrong), 94c1ead (anisotropic heap
  samplers keep point mips), e04c078 (a program's constant table without its
  instructions); fxceless f6a09d3 (the fused multiply-add of the console: water
  reflections) and 06702fe (BC7 rotation); DexNom 76749ce (a wave64 mask restored: one
  more program translated); cool_fox's fix of the tracer at a join.
- The PC's memory: nothing but avatarmanz's switch above and xcl 28946bb1 (a big
  compile waits for commit headroom: one program of 29.8 MB had taken the driver past
  14 GB).

**Measured elsewhere and no gain** (not to be done again): fxceless's branch
`stretch-port` (`VICTORY_STRETCH`: windows of view memory placed on a view of ours, 1
and 9 windows placed in two runs; here 0 of 789 windows are one run of a section and
426, 1,740 MB, lie over unmapped pages: run ram-map1), its `VICTORY_LIGHT_REFRESH`,
`VICTORY_HEAP_LOCKS` and `VICTORY_TABLE_STAGES`; xcl's heaps in the card's memory
(reads over the bus some thirty times slower) and its console FSR 4 (11 ms a frame on
an RX 9070); DexNom after its bridge has README commits only; Belobug `main` is its
older line (5 to 7 fps), Leonida does not start the title, and the remotes of cool_fox
and Leonida are gone.
