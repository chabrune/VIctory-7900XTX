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

## The queue of textures picked by index, in parts, 2026-11-26

The submitting thread stands blocked for 40 % of its wait on one lock: the queue of
slots whose texture view waits to be brought up to date (`descriptor::host::TextureQueue`),
which the threads that translate lists take for every texture view they write (4.9 ms of
a 33 ms frame). It is now in 16 parts, each with a lock of its own, by slot
(`VICTORY_TEXTURE_SHARDS`; 1: one lock for all of it, as before).

What does not change is the order textures are served in: `due` picks them by the
number of their write, and that number comes from one counter over all the parts, so
the newest-first/oldest-next turns of the backlog come out as before (the tests of the
queue, `the_texture_queue_serves_the_newest_writes_first` among them, run on the parts).
What changes is who waits for whom: two threads writing slots of different parts no
longer wait for each other, and the pass that takes the due slots once a frame locks
each part in turn instead of the one lock every write of the frame went through. A bulk
look at a list of slots (a page written under a texture) is one lock a part, not one a
slot.

Not measured yet on the title: `VICTORY_TEXTURE_SHARDS=1` is the A/B of the same run.

## What is next

1. The preparing thread: split what one submission does over two threads in a row; ask
   for the next submission while this one is sent; view the card's own buffers where fills
   copy them now. (The queue of textures picked by index no longer has one lock for the
   frame: see above.)
2. The card: small buffers' copies in its own memory (`VICTORY_CARD_BUFFERS_MIN_KB=64`,
   after fxceless c36971b2; not measured here yet), and the passes that run for nothing
   without rays.
3. Stalls: 270-660 s of a session were full of frames over 200 ms, almost all while new
   pipelines were made. `victory warm`, and a quarter of the workers for the slow ones.
   (The ~830 MB the small buffers of their own gave back are enough for 2 GiB of the
   title's windows in card memory beside the same textures: not measured yet.)
4. Outdoor flicker: suspected to be one bad indirect record that uses up its list's
   budget, so that every later draw of it is empty. The log's frame line counts them;
   `VICTORY_INDIRECT_REST=0` is the experiment.
5. Memory, 40-46 GB private: lists' allocators 9 GB, windows 7, uploads 4
   (`VICTORY_MEMORY_MAP=1` lists it).

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
