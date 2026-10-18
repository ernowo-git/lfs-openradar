# node-insim tyre thickness check

Checked 17 October 2026. Scope: whether [simbroadcasts/node-insim](https://github.com/simbroadcasts/node-insim) supplies remaining tyre thickness, tread depth or wear, especially through OutSim.

## Finding

**No remaining tyre thickness, tread depth or wear field was found in the checked source.** Its extended OutSim wheel decoder exposes the same categories of data as the documented LFS wheel structure: suspension, steering, forces, vertical load, angular velocity, lean, air temperature, slip and ground contact. Neither `OutSimWheel` nor the surrounding extended packet contains thickness or wear. This confirms the earlier OutSim finding rather than providing a new measurement channel. [Wheel decoder](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/out/OutSimWheel.ts#L4-L32), [extended packet](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/out/OutSimPack2.ts#L25-L76)

## What the tyre-related fields mean

| Fields | Meaning and units | Availability |
| --- | --- | --- |
| `OutSimWheel.AirTemp` | Tyre air temperature; one byte, degrees Celsius | Extended OutSim wheel block |
| `OutSimWheel.SlipFraction`, `SlipRatio`, `TanSlipAngle` | Slip values, including a 0–255 fraction representation; no depth unit | Extended OutSim wheel block |
| `IS_NPL.TyreRL/RR/FL/FR` | Tyre compound enum, not a wear reading | Player joining or leaving pits; requestable player information |
| `IS_NPL.RWAdj`, `FWAdj` | Rear/front tyre width reduction configuration, low four bits | Player information, not tread loss |
| `IS_PIT.TyreRL/RR/FL/FR` | Compound identifiers for tyre changes; enum value 255 means unchanged | Pit-stop event |
| `IS_SET.RTyrePressLeft/Right`, `FTyrePressLeft/Right` | Setup tyre pressure in kPa | Shared setup received with the appropriate InSim flag |
| `IS_SET.RTyreWarmer`, `FTyreWarmer` | Setup tyre-warmer settings for hotlaps | Shared setup; not current tread temperature or wear |

Sources: [wheel fields](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/out/OutSimWheel.ts#L20-L31), [new-player packet](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/packets/IS_NPL.ts#L7-L75), [pit-stop packet](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/packets/IS_PIT.ts#L6-L47), [compound enum](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/packets/enums/TyreCompound.ts#L1-L28), [setup delivery](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/packets/IS_SET.ts#L8-L19), [rear setup fields](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/packets/IS_SET.ts#L153-L187), [front setup fields](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/packets/IS_SET.ts#L212-L246).

The wheel block is selected by `OSO_WHEELS = 128` (`0x80`); setting all documented options (`0x1ff`) still does not add thickness. The library chooses between legacy and extended OutSim decoding based on its configured `OutSimOpts`; it is decoding the available UDP fields rather than obtaining an additional internal tyre measurement. [Option flags](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/out/OutSimOptions.ts#L1-L12), [extended packet options](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/out/OutSimPack2.ts#L16-L23), [decoder selection](https://github.com/simbroadcasts/node-insim/blob/ba29cf8675a7114ce27baf2caecc9fc635ba784d/src/out/OutSim.ts#L89-L94)

## Verification boundary

Inspected a direct shallow clone of `master` at commit `ba29cf8675a7114ce27baf2caecc9fc635ba784d`; searched its implementation and bundled `reference/InSim.h` for tyre, thickness, tread, wear, rubber and radius terms. The cited URLs pin the inspected revision. No live LFS packet capture, Node.js runtime check or tyre-wear calibration was performed. The absence claim applies to this source revision and the fields it implements, not to undocumented game internals or future protocol changes.

For OpenRadar, this repository supplies no basis for labelling any calculated wear or thickness value as measured telemetry. Existing supported tyre-air readings and any proposed wear estimate must remain distinct. See the broader [tyre condition research](tyre-condition-gadget-research.md).
