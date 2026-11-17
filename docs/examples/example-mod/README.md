# Example mod

A mod folder showing how a mod presents itself in TA Reborn's launcher: a banner, an animated
gameplay preview, an icon, screenshots, tags and links, all named in its [`mod.toml`](mod.toml).
It changes nothing in the game.

To see it, copy this folder into one of your mod folders (the launcher's **MODS** page, **MOD
FOLDERS…**) and press **RESCAN**. Its card shows its banner, its preview plays when the pointer
rests on the card, and its page shows the rest. **MOD INFO** checks its presentation along with
its content.

The pictures are original art, drawn by [`tools/examples/example_mod_art.py`](../../../tools/examples/example_mod_art.py);
use them as a starting point for your own. The whole format is in
[docs/MODDING.md](../../MODDING.md#presenting-a-mod-in-the-launcher).
