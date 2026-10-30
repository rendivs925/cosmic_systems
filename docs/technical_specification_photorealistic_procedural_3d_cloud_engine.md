# Technical Specification: Photorealistic Procedural 3D Cloud Engine

## 1. Executive Summary & Objective

This document provides a comprehensive technical specification and algorithmic framework for constructing a high-performance, photorealistic procedural 3D cloud engine suitable for real-time graphics engines (WebGL / Three.js, Unreal Engine, Unity, Custom GLSL/HLSL Pipelines) and offline rendering pipelines.

The goal is to move past naive volumetric solutions—such as flat billboard particle clouds, low-resolution 2D heightmap blurs, and simple homogeneous noise fog—and establish a physically grounded, real-time volumetric ray marching architecture capable of simulating cumulus, stratocumulus, and altocumulus clouds with realistic light transport, silver linings, internal multiple scattering, and wind dynamics.

---

## 2. Naive vs. Photorealistic Procedural Architecture

| Feature Area | Naive / Legacy Approach | Photorealistic Procedural Standard | 
| ----- | ----- | ----- | 
| **Geometry & Volume** | Standard 2D billboards, intersecting planes, or static bounding meshes. | Dynamic atmosphere shell bounded ray marching over 3D noise volume functions ($100\,\text{km}$ planar or spherical planetary shell). | 
| **Density Noise** | Single-octave Perlin noise or uncompressed 2D heightmaps. | Composite **Perlin-Worley 3D Noise** combined with high-frequency 3D Worley erosion and curl-noise turbulence. | 
| **Light Transport** | Single Lambertian or Blinn-Phong directional shading applied to density. | **Beer-Lambert Extinction** + **Triple-Lobe Henyey-Greenstein Phase Function** + **Diffuse Multiple Scattering** + **Multiple Scattering Approximation**. | 
| **Volumetric Sampling** | Fixed-step ray marching with severe aliasing and slice artifacts. | Adaptive step-size ray marching with screen-space stochastic jittering, distance attenuation, and temporal reprojection (quarter-res rendering). | 
| **Cloud Morphometry** | Static cloud density profiles without vertical variation. | Altitude-dependent density curves driven by a dynamic 2D **Weather Map** (Coverage, Precipitation, Cloud Height Type). | 

---

## 3. Volumetric Noise & Density Field Generation

### 3.1 Perlin-Worley Base Density Function

To represent the billowy yet structured nature of cloud formations, the primary volume density field combines smooth continuous directional noise (**Perlin Noise**) with cellular fractal noise (**Worley Noise**).

1. **Perlin-Worley Hybrid Mapping:** Using a 3D grid coordinate $\vec{p} = (x, y, z)$, define the low-frequency base density $\text{Noise}_{\text{base}}(\vec{p})$ by mapping 3D Perlin noise using 3D Worley noise as an inverted threshold filter:

   $$
   \text{Noise}_{\text{base}}(\vec{p}) = \text{remap}\left( N_{\text{Perlin}}(\vec{p}), \, N_{\text{Worley, fbm}}(\vec{p}) - 1.0, \, 1.0, \, 0.0, \, 1.0 \right)
   $$

   Where the remapping equation is defined as:

   $$
   \text{remap}(v, l_1, h_1, l_2, h_2) = l_2 + \frac{v - l_1}{h_1 - l_1} \cdot (h_2 - l_2)
   $$

2. **Multi-Octave Fractional Brownian Motion (fBm):** Worley noise uses multiple harmonic frequencies to form realistic cauliflower-like billows:

   $$
   N_{\text{Worley, fbm}}(\vec{p}) = \sum_{m=0}^{M-1} w_m \cdot \text{Worley}\left( \vec{p} \cdot f_0 \cdot 2^m \right)
   $$

   Where weights $w_m = 0.5^{m+1}$, initial frequency $f_0$, and octave count $M = 3$.

### 3.2 High-Frequency Detail Erosion

To simulate fine turbulent edges and wisps without consuming massive memory, compute a secondary 3D texture containing three octaves of high-frequency Worley noise $N_{\text{detail}}(\vec{p}) = (W_1, W_2, W_3)$ and erode the base density field along its boundaries:

$$
\rho_{\text{eroded}}(\vec{p}) = \text{remap}\left( \text{Noise}_{\text{base}}(\vec{p}), \, N_{\text{detail}}(\vec{p}) \cdot (1.0 - h), \, 1.0, \, 0.0, \, 1.0 \right)
$$

Where $h \in [0, 1]$ is the normalized relative height within the cloud layer.

### 3.3 Height Gradient Profile Function $HG(z)$

Clouds vary structurally across vertical altitude. Define a height gradient factor $HG(z, T)$ using Lifted Condensation Level (LCL) curves based on cloud type $T$ ($0 = \text{Stratus}$, $0.5 = \text{Stratocumulus}$, $1.0 = \text{Cumulus}$):

$$
HG(z, T) = \text{clamp}\left( \frac{z - z_{\text{bottom}}}{z_{\text{top}} - z_{\text{bottom}}}, \, 0, \, 1 \right)
$$

* **Stratus ($T \le 0.25$):** Flat density layer that quickly attenuates near top and bottom.
* **Cumulus ($T \ge 0.75$):** Narrow flat base, expanding billows in the middle ($z \approx 0.4 \to 0.7$), rapidly tapering wispy tops.

$$
\text{Density}(\vec{p}) = \rho_{\text{eroded}}(\vec{p}) \cdot HG(z, T) \cdot W_{\text{coverage}}(\vec{p}_{xz})
$$

### 3.3.1 Regional Cloud Morph

A deck with one morph everywhere is one cloud type: the whole planet is drawn
with the same sheet, or the same cell field, and a flight view looks identical
wherever it is. Morph is therefore a field, not a constant.

Two weather-map channels drive it, and both are histogram-equalised to a rank
(section 3.5.1), so a threshold selects a *share* of the deck rather than a
slice of one narrow band:

- **Type rank (blue).** `0` is a flat stratus sheet, `1` a convective cell
  field. The deck interpolates its own `cloud_type` towards the rank by
  `morph_variation`, which is `1` for the water deck and `0` for the cirrus
  veil, because ice at one altitude has one shape. Features span tens to a few
  hundred kilometres: a cloud regime is mesoscale, so a single 100 km view
  normally holds one regime and a wide view holds several.
- **Convective-vigour rank (green).** The top tenth of the map is deep
  convection and the third above `0.55` is broken cumulus. There the vertical
  profile is forced to its fullest, so towers rise out of the deck where the
  weather is actually converting.

Both are baked as ranks because a raw fBm concentrates almost every texel near
its mean: a morph threshold against it moves between "all sheet" and "all
cells" over a few percent of the range.

**Rejected: a spatially varying cell frequency.** Scaling the noise coordinate
by a per-sample factor (bigger cells in sheets, smaller in cells) is the obvious
way to widen the shape range, and it is visually wrong: a spatially varying
frequency shears the field, and the deck develops visible swirl and marbling
along the regime boundaries. Any per-sample change to the *sample coordinate* is
therefore off the table; the remaining avatar of shape diversity is content, not
geometry — a sharper, multi-scale density field (section 3.2) and the top-height
field (section 4.3.2.1).

### 3.4 Wind Dynamics & Curl-Noise Advection

Advect cloud structures over time $t$ along wind vector $\vec{V}_{\text{wind}}$ with a continuous 3D velocity field derived from **Curl Noise** to prevent uniform sliding:

$$
\vec{p}_{\text{sampled}} = \vec{p} + t \cdot \vec{V}_{\text{wind}} + \left( \nabla \times \vec{\Psi}(\vec{p}) \right) \cdot A_{\text{turb}}
$$

Where $\vec{\Psi}$ is a 3D potential field composed of simplex noise, guaranteeing mass-conserving, divergence-free incompressible flow ($\nabla \cdot \vec{v} = 0$).

Curl-noise advection is not implemented in the production renderer: advection is
a uniform translation along the measured wind vector. That is adequate at the
current feature sizes and presentation clock, but it is a known divergence from
this specification.

### 3.4.2 Sampling Level of the Density Field

The density volume is sampled at an explicit mip level derived from the march
step, `l = max(0, log2(step / cell))`, where `cell = 1 / (6 * noise_scale)` is
one base cell. Two reasons.

- A step kilometres long cannot resolve a four-kilometre cell. Sampling the
  finest level there aliases the field, which is what a ray that reaches the
  limb takes: the segment through the deck is hundreds of kilometres long and
  the 96 primary steps are kilometres apart.
- `textureSample` uses screen-space derivatives, and the march loop is
  divergent. Derivative-based sampling inside non-uniform control flow is
  undefined in WGSL, so the level a step picked was not well defined even in
  principle.

The weather map keeps level zero: its features are tens to hundreds of
kilometres, so even a kilometre step resolves it.

This is a correctness and future-proofing change, not a visible one: at a 40 km
chase view the deck's steps are tens of metres and the level is zero, so the
frame is unchanged to one 8-bit step.

### 3.4.1 Cell Scale and Street Alignment

Two properties of a boundary-layer deck come from the geometry of the layer
rather than from the noise, and both were missing from the first renderer.

**Cell diameter follows the layer depth.** Convective cells in a layer of depth
$H$ have an aspect ratio of order one, so a deck's cell size is not a free
constant. With a base volume that holds six cells per tile, the noise frequency
is $f = 1 / (6 H)$: the tile spans six layer depths and one cell is one layer
deep. The previous fixed 4.5 km tile put Earth's cells at 750 m across, and from
altitude that read as uniform popcorn. Sizing cells from the slab also fixes a
second problem: coverage thresholds the *density*, so a low-coverage region
survives only where the noise peaks, and peaks of an undersized field are
undersized clouds.

**Cells and streets align with the wind.** Convection organizes into rolls
aligned with the boundary-layer wind, whose spacing is a few layer depths. The
density field is sampled through a coordinate compressed by a factor
$s \approx 2.4$ along the wind, which stretches every feature by $s$ along it.
As a linear map of the sample coordinate this keeps the volume seamless: its
period simply grows by $s$ along the wind. The compression is horizontal in
body-fixed Cartesian coordinates, i.e. in the equatorial plane, which is the
same global-wind approximation the uniform advection already makes; it does not
turn with latitude.

Both changes live in the shared density module, so the deck, the ground shadow,
and the sea's reflected deck still sample one field.

### 3.5 Weather-Map Tiling and Advection

The weather map is a periodic 2D field in body-fixed equirectangular
coordinates. Sampling it once across the sphere puts every feature at synoptic
scale (thousands of kilometres), so a flight view sees a single coverage value.
The renderer samples it at an integer tiling factor of 48, which adds regional
structure while keeping the longitude and pole seams closed; the generator's
cell period equals its per-octave frequency so the map wraps exactly.

Coverage and the 3D density field must advect at the same measured wind speed.
For a body-fixed map, one UV unit spans the planet's circumference, so the drift
in UV per second is

$$
\Delta u = \frac{V_{\text{wind}}}{2 \pi R}.
$$

The earlier fixed `1.0e-6` coefficient ignored the wind speed and drifted at
about 40 m/s, which sheared the coverage pattern away from the cloud shapes over
time.

### 3.5.1 Coverage as a Sky Fraction

$W_{\text{coverage}}$ must select a *share of the sky*, not scale the density.
A measured regional cloud fraction $c$ (monthly mean, from the climate package)
is turned into the local coverage by thresholding the weather map's rank field
$r$:

$$
W_{\text{coverage}} = \operatorname{saturate}\!\left(
\frac{r - (1 - c)}{\max(c, 0.02)}\right).
$$

Below $1 - c$ the air is genuinely clear; above it the band ramps from a wispy
edge to the dense core of a system. This requires $r$ to be **uniform**, so the
generator histogram-equalises the coverage channel at bake time. Scaling
instead (`c \cdot r \cdot 1.4`, the previous form) fails because an fBm
concentrates almost every texel near its mean: the product saturates over most
of the map and the deck becomes a planet-wide blanket, which is exactly what a
rank threshold fixes.

The cirrus veil uses the same threshold against a much smaller fraction
(0.55 $\times$ the surface cloud fraction, clamped to $[0.05, 0.45]$) and a
cirrus optical depth of $\tau \approx 1$: at a water-deck extinction of
$0.008\ \mathrm{m^{-1}}$ the 1.6 km veil integrated to $\tau \approx 7.7$ and
rendered as an opaque sheet.

### 3.6 Layered Decks

The production renderer draws two spherical slabs from the same shader: a
boundary-layer water deck and a thin high cirrus veil. Both are camera-centred
proxies, so their transparent draw order cannot come from view distance; the
renderer sets each material's sort bias every frame from the camera's
planet-centric radius and draws the farther slab first.

---

## 4. Atmospheric Light Transport & Scattering Physics

Light entering a cloud volume undergoes extinction (absorption + out-scattering) and in-scattering.

```
       Sun Light (L_sun)
             \
              \
               v
  Camera <--- (P)  [Scattering Point]
      \       /
       \     /
    Ray Marching (In-scattering)
```

### 4.1 Beer-Lambert Law for Transmittance

The optical depth $\tau$ along a path $S$ through a volume with extinction coefficient $\sigma_e$ determines transmittance $T$:

$$
\tau(a, b) = \int_a^b \sigma_e(r(t)) \, dt
$$

$$
T(a, b) = \exp\left( -\tau(a, b) \right)
$$

In discretized form along ray steps $\Delta s$:

$$
T_i = T_{i-1} \cdot \exp\left( -\sigma_e \cdot \rho(\vec{p}_i) \cdot \Delta s \right)
$$

### 4.2 Phase Function $P(\theta)$ (Forward Silver-Lining & Backward Scattering)

Water droplets in clouds produce strong forward Mie scattering (creating bright rims around the sun) alongside minor backward scattering.

Use a **Triple-Lobe Henyey-Greenstein (HG) Phase Function**:

$$
P_{\text{HG}}(\theta, g) = \frac{1 - g^2}{4\pi \left( 1 + g^2 - 2g \cos\theta \right)^{3/2}}
$$

$$
P_{\text{triple}}(\theta) = (1 - w_1 - w_2) \cdot P_{\text{HG}}(\theta, g_{\text{forward}}) + w_1 \cdot P_{\text{HG}}(\theta, g_{\text{backward}}) + w_2 \cdot P_{\text{HG}}(\theta, g_{\text{side}})
$$

* **Recommended Parameters:** $g_{\text{forward}} \approx 0.82$, $g_{\text{backward}} \approx -0.35$, $g_{\text{side}} \approx 0.10$, blending weights $w_1 = 0.25$, $w_2 = 0.15$.
* $\theta$ is the angle between the sun direction vector $\vec{L}$ and the view ray direction vector $\vec{V}$ ($\cos\theta = \vec{L} \cdot \vec{V}$).

### 4.3 Single-Scatter Step Integral

The radiance scattered toward the camera from one step is the incoming radiance
times the droplet single-scattering albedo and the analytic optical-depth
integral over that step. With `sigma_e = rho * absorption` and step length
`Delta s`:

$$
L_i = \omega_0 \cdot L_{\text{in}} \cdot \left(1 - \exp(-\sigma_e \cdot \Delta s)\right)
$$

$$
L_{\text{in}} = L_{\text{sun}} \cdot T_{\text{beam}} \cdot T_{\text{multi}} \cdot P_{\text{triple}}(\theta) + L_{\text{diffuse}}
$$

`T_beam` is the atmosphere's per-channel transmittance of the direct beam at the
deck altitude (`AtmosphericOptics::sun_transmittance`), which reddens and dims
the sunlight as the Sun approaches the horizon. The diffuse term keeps the
unattenuated irradiance because it is sky light rather than the beam.

with `omega_0 = 0.99` for water droplets. The accumulated in-scatter is
`L += T_i * L_i`, and the transmittance is updated with the same step.

Two consequences matter:

* The source radiance is per unit *optical depth*, so there is no division by
  `sigma_e` and the result cannot depend on how the optical depth is split
  between density and the extinction coefficient.
* Optically thin cloud is dimmed automatically by `1 - exp(-tau)`. A separate
  artistic "powder" multiplier is not needed, and the earlier version of this
  specification used the *sun-path* density for it, which darkened exactly the
  sunlit tops. That term has been removed.

### 4.3.1 Solar Terminator Gate

Direct sunlight reaches a cloud sample only while the Sun is above that sample's
local horizon. The geometric horizon at cloud altitude sits slightly below the
tangent plane,

$$
\cos\theta_{\text{horizon}} = -\sqrt{1 - \left(\frac{R}{R + h}\right)^2},
$$

about `-0.03` for a 3 km cloud, so the gate opens a little below zero and fades
through civil twilight:

$$
G = \operatorname{smoothstep}(-0.05, 0.02, \hat{n} \cdot \hat{L}).
$$

The direct term is multiplied by `G`; the diffuse term uses
`max(0, n·L)` so it also vanishes at night. Without the gate the sun march
reports a clear path for a below-horizon Sun and lights the night-side deck with
the full solar irradiance.

### 4.3.2 Diffuse Multiple Scattering

Cloud droplets are nearly conservative scatterers, so an optically thick deck
reflects most of the sunlight it intercepts as a near-Lambertian layer. Rather
than only attenuating the direct term, add the diffuse radiance implied by the
layer reflectance:

$$
L_{\text{diffuse}} = \omega_0 \cdot \frac{E_{\text{sun}}}{\pi} \cdot \cos\theta_{\text{sun}} \cdot c(h)
$$

where `cos(theta_sun)` is the cosine between the local vertical and the sun
direction, and `c(h)` is a height tint: white at the sunlit top of the deck and
the ground-bounce colour at its base. Because the step integral weights this
source by `1 - exp(-sigma_e ds)`, a thin wisp reflects proportionally less, and
the term vanishes at night and across the terminator. This replaced the fixed
`ambient_fraction` scalar.

`c(h)` is a tint, not a shape term. The expression is the radiance of an
*infinite flat* deck, so every dome receives the same diffuse light no matter
which way it faces, and a deck drawn with it alone reads flat from above: the
cloud tops sit at one brightness and only the phase function and the silhouette
separate one cell from the next.

Two cheaper shape terms were measured against the rendered deck and rejected.

- **Density-gradient surface normals.** The outward normal of the iso-density
  surface is `-normalize(grad rho)`, but at the visible surface the vertical
  taper of the height profile dominates the horizontal noise gradient, so the
  normal is within about ten degrees of "up" everywhere and the Lambert ratio
  `n . sun / cos(sun zenith)` is 1.0 to render precision. Cell tops really are
  flat iso-density surfaces here: the profile, not the noise, decides where the
  deck ends.
- **Diffuse self-shadowing from the Sun march.** A point on the visible surface
  is where the view transmittance first falls, and above it the deck is
  optically thin, so the Sun march returns its full clear-sky value there. The
  march does see 50-300 density-metres from *deeper* samples, but those are
  behind the visible surface and contribute almost nothing to the image. Using
  the deeper value would make the cloud's brightness follow the view depth, the
  same view-dependent artifact that the old powder term was removed for.

Both failures point at one thing: relief needs the *top height field* of the
deck, either by marching the column above each sample down to a transmittance
threshold and differentiating it horizontally, or by replacing the slab term
with a multiple-scattering integral that actually transports between cells.

### 4.3.2.1 Aerial Perspective of the Deck

Without extinction between the deck and the camera, a cloud on the limb is as
bright as one overhead and the deck reads as a cut-out pasted on the sky. The
correct composition is not a dimming: the air in front *replaces* the cloud's
light with its own,

$$
L = T \cdot L_{\text{cloud}} + (1 - T) \cdot L_{\text{haze}},
\qquad T = e^{-\tau},
$$

with `\tau` the optical depth of the air between the camera and the cloud and
`L_haze` the sky radiance the deck is seen against. Because the output is
premultiplied, the term the shader adds is `(1 - T) * opacity * L_haze`: where
the cloud is transparent the background sky already carries the haze.

**Optical depth, not distance times extinction.** The vertical optical depth of
the air between the camera and the deck comes from the shared `AtmosphericOptics`
(`overhead_optical_depth(camera) - overhead_optical_depth(deck)`); the shader
divides it by the cosine of the view zenith angle to get the path's air mass,
capped at 12.5 near the tangent. A mean extinction coefficient multiplied by the
slant range is not equivalent: the slant path leaves the dense layer, so a long
path is not simply a longer version of the vertical one.

**The haze is the sky, not the downward airlight.** `L_haze` is
`sky_radiance_direction` evaluated along the *mirrored* view direction — the sky
the deck is seen against, lit by the same sun. Using the downward path's own
in-scatter instead gives the air below the deck, which at a low sun is reddened
and does not converge on the sky at the seam: the band stays warm while the sky
above it is pale, so the cut-out is dimmed but not removed.

Measured at the 40 km chase view, comparing the original and the two haze
variants over the deck/sky seam (mean of the largest adjacent-row jumps along 32
columns):

| Variant | Luminance jump | Colour jump |
| --- | --- | --- |
| no aerial perspective | 0.412 | 0.682 |
| distance x mean extinction, downward airlight | 0.308 | 0.546 |
| air-mass optical depth, mirrored sky | **0.258** | **0.475** |

The near deck is almost unchanged (red +3%, blue -5%) because its air mass is
about one; the far deck loses about a tenth of its luminance and takes the sky's
colour. That is the depth cue the deck was missing.

### 4.3.3 Terrain Cloud Shadows

The ground receives the deck's shadow from the same density field, not a
separate proxy. Both shaders import `shaders/cloud_density.wgsl`, which owns
`CloudField`, the weather-map projection, the height profile, and the density
sampler; the cloud material builds a `CloudField` from its larger uniform and
the terrain material carries the field plus the inertial Sun direction in its
own uniform.

For a ground fragment inside both deck spheres the Sun ray crosses the slab
between the far root of the base sphere and the far root of the top sphere:

$$
t_{\text{in}} = -b + \sqrt{b^2 - c_{\text{base}}}, \qquad
t_{\text{out}} = -b + \sqrt{b^2 - c_{\text{top}}},
$$

with $b = \hat{p} \cdot \hat{L}$ and $c_r = |p|^2 - r^2$, both evaluated in the
body-fixed frame. Marching $N = 6$ midpoint samples across
$[t_{\text{in}}, t_{\text{out}}]$ gives the beam optical depth

$$
\tau = \sum_{i=1}^{N} \sigma_e(p_i) \, \Delta s, \qquad
\sigma_e = \rho \, \kappa,
$$

using the deck's own extinction $\kappa$, so the shadow transmittance
$T = e^{-\tau}$ matches the opacity of the cloud above it. Only the direct-sun
term is scaled; ambient sky fill is preserved. The terrain ground material and
the water material share this march, its `CloudShadow` uniform (field, Sun, and
inertial-to-body rotation), and the density module, so land and sea are shaded
by the same deck.

An optically thick deck drives $T \to 0$, but an overcast day still delivers
about a fifth of the clear-sky irradiance as diffuse skylight. The ground term
therefore applies

$$
V = \operatorname{mix}(0.2, 1, T),
$$

a documented presentation approximation that stands in for the deck's
hemisphere radiance, which is not injected into the scene ambient.

### 4.3.4 Sea Mirror Reflection

A water surface is a dielectric mirror, so a grazing sea returns a growing share
of whatever is above it. The sea evaluates the same $[t_{\text{in}},
t_{\text{out}}]$ march along the view's mirror direction $\hat{R} =
\operatorname{reflect}(-\hat{V}, \hat{N})$ to obtain the deck's opacity on that
ray, then reflects

$$
L_{\text{refl}} =
\operatorname{mix}\left(L_{\text{deck}},\; L_{\text{sky}}(\hat{R}),\; T_R\right),
\qquad
L_{\text{deck}} = \omega_0 \frac{E_\odot}{\pi} T_\odot \cos\theta_\odot,
$$

where $E_\odot$ is the top-of-atmosphere solar irradiance, $T_\odot$ the
atmospheric beam transmittance above the deck, and $\cos\theta_\odot$ is gated
on the Sun's local horizon. The forward-scattering lobe is omitted because a
water cloud's diffuse transmission dominates its directed lobe when viewed from
below. $L_{\text{sky}}$ is the single-scatter integral in
`sky_radiance.wgsl`, imported by both the dome and the sea so the reflected sky
is the sky being drawn.

The reflected term is weighted by the dielectric Fresnel reflectance

$$
F(\theta) = F_0 + (1 - F_0)\left(1 - \cos\theta\right)^5,
\qquad F_0 = \left(\frac{n - 1}{n + 1}\right)^2 = 0.02,
$$

for $n = 1.33$, and added as the difference over the uniform ambient specular
the PBR path already reflects, so the surface never gains energy twice.

### 4.4 Multiple Scattering Approximation (Octave-Based Attenuation Decay)

Single-scattering models render cloud centers too dark. To simulate multiple internal bounces without expensive unbiased path tracing, accumulate scattering across $K$ attenuation octaves with decreasing density scale and increasing forward scattering anisotropy:

$$
T_{\text{multi}}(\tau_{\text{sun}}) = \sum_{k=0}^{K-1} c_k \cdot \exp\left( -\tau_{\text{sun}} \cdot a^k \right),
\qquad \tau_{\text{sun}} = \int_{\text{path}} \rho \, \sigma_e \, ds
$$

* **Constants:** $a = 0.5$ (extinction reduction factor per bounce), $c_k = 0.5^k$ (energy conservation attenuation factor per octave).

---

## 5. Volumetric Ray Marching Algorithm

The renderer casts a ray from camera origin $\vec{O}$ along normalized direction $\vec{D}$ into the cloud bounding volume $[t_{\text{entry}}, t_{\text{exit}}]$.

```
Camera (O) ----------------> t_entry [ Outer Atmosphere Boundary ]
                               |   .   .  (Ray March Step ds)
                               |  .  density > 0  .
                               |   .   .   .   .
                             t_exit [ Inner/Upper Boundary ]
```

### 5.1 Bounding Shell Intersection

Compute ray intersections with two concentric spherical shells representing the cloud floor $R_{\text{inner}} = R_{\text{planet}} + h_{\text{bottom}}$ and cloud ceiling $R_{\text{outer}} = R_{\text{planet}} + h_{\text{top}}$:

$$
t_{\text{shell}} = -(\vec{O} \cdot \vec{D}) \pm \sqrt{(\vec{O} \cdot \vec{D})^2 - \left( |\vec{O}|^2 - R^2 \right)}
$$

### 5.2 Ray Step Jittering & Dynamic Acceleration

To eliminate slice-aligned banding artifacts during ray marching:
1. Offset the initial ray starting distance using a 2D blue noise texture $R_{\text{blue}}(x, y)$:

   $$
   t_0 = t_{\text{entry}} + \Delta s \cdot R_{\text{blue}}(\text{screen}_x, \text{screen}_y)
   $$

2. **Adaptive Empty-Space Skipping:** If sampled density $\rho(\vec{p}) < \epsilon$, increase step size $\Delta s \leftarrow 1.8 \cdot \Delta s$. When $\rho(\vec{p}) \ge \epsilon$, return to primary step resolution $\Delta s_0$.

---

## 6. Complete Production GLSL Shader Implementation

The following complete fragment shader implements volumetric ray marching, 3D
Perlin-Worley density evaluation, multi-octave light transport, phase functions,
and atmospheric fog blending.

> **Implementation note.** The production renderer is
> `assets/shaders/clouds.wgsl`, bound to `CloudParams` in
> `src/infrastructure/bevy_adapters/rendering/materials.rs`. It follows sections
> 4.1-4.4 and 5 but has since diverged from this appendix in four ways: it emits
> premultiplied HDR radiance instead of tone mapping, uses interleaved-gradient
> noise instead of a blue-noise texture, has no aerial-haze blend (the shared
> atmospheric fog handles aerial perspective), and draws two decks (a water deck
> and a high cirrus veil). Treat this appendix as the algorithmic reference, not
> a literal listing.

```glsl
precision highp float;
precision highp sampler3D;

varying vec2 vUv;
varying vec3 vWorldPosition;
varying vec3 vRayDirection;

// Uniform Inputs
uniform vec3 uCameraPos;
uniform vec3 uSunDir;
uniform vec3 uSunColor;
uniform vec3 uSkyColor;
uniform vec3 uGroundColor;

uniform float uTime;
uniform float uCoverage;
uniform float uCloudType;
uniform float uDensityScale;
uniform float uErosionScale;
uniform float uAbsorption;
uniform float uForwardG;
uniform float uWindSpeed;
uniform float uWindDirectionX;
uniform float uWindDirectionZ;
uniform float uMultiScatteringOctaves;
uniform float uBottomRadius;
uniform float uTopRadius;

uniform int uPrimarySteps;
uniform int uLightSteps;

uniform sampler3D uBaseNoise;
uniform sampler3D uDetailNoise;
uniform sampler2D uWeatherMap;

uniform vec3 uBoxMin;
uniform vec3 uBoxMax;

const float PI = 3.14159265359;

// Remap Utility Function
float remap(float val, float oldMin, float oldMax, float newMin, float newMax) {
    return newMin + ((val - oldMin) / (oldMax - oldMin)) * (newMax - newMin);
}

// Ray Box Intersection
vec2 rayBoxIntersection(vec3 rayOrigin, vec3 rayDir, vec3 boxMin, vec3 boxMax) {
    vec3 invDir = 1.0 / rayDir;
    vec3 t0 = (boxMin - rayOrigin) * invDir;
    vec3 t1 = (boxMax - rayOrigin) * invDir;
    vec3 tmin = min(t0, t1);
    vec3 tmax = max(t0, t1);
    float tNear = max(max(tmin.x, tmin.y), tmin.z);
    float tFar = min(min(tmax.x, tmax.y), tmax.z);
    return vec2(tNear, tFar);
}

// Single Henyey-Greenstein Phase Lobe
float hgPhase(float cosTheta, float g) {
    float g2 = g * g;
    return (1.0 - g2) / (4.0 * PI * pow(max(0.0001, 1.0 + g2 - 2.0 * g * cosTheta), 1.5));
}

// Triple-Lobe Phase Function (Forward Silver Lining + Backscatter + Fill)
float tripleLobePhase(float cosTheta, float g1) {
    float forwardLobe = hgPhase(cosTheta, g1);
    float backwardLobe = hgPhase(cosTheta, -0.35);
    float sideLobe = hgPhase(cosTheta, 0.1);
    return mix(mix(forwardLobe, backwardLobe, 0.25), sideLobe, 0.15);
}

// Altitude Density Profile
float densityHeightProfile(float heightFraction, float type) {
    float stratus = remap(heightFraction, 0.0, 0.1, 0.0, 1.0) * remap(heightFraction, 0.15, 0.35, 1.0, 0.0);
    stratus = clamp(stratus, 0.0, 1.0);

    float cumulus = remap(heightFraction, 0.0, 0.15, 0.0, 1.0) * remap(heightFraction, 0.45, 0.9, 1.0, 0.0);
    cumulus = clamp(cumulus, 0.0, 1.0);

    return mix(stratus, cumulus, type);
}

// Volumetric Density Sampler
float sampleDensity(vec3 p) {
    vec3 normPos = (p - uBoxMin) / (uBoxMax - uBoxMin);
    if (normPos.x < 0.0 || normPos.x > 1.0 || normPos.y < 0.0 || normPos.y > 1.0 || normPos.z < 0.0 || normPos.z > 1.0) {
        return 0.0;
    }

    float heightFraction = normPos.y;

    vec2 weatherUv = normPos.xz * 0.4 + vec2(uTime * 0.004 * uWindSpeed);
    vec4 weather = texture2D(uWeatherMap, weatherUv);
    
    float globalCoverage = clamp(uCoverage * weather.r * 1.4, 0.0, 1.0);
    float cloudTypeVal = clamp(mix(uCloudType, weather.b, 0.5), 0.0, 1.0);

    if (globalCoverage < 0.01) return 0.0;

    float heightGradient = densityHeightProfile(heightFraction, cloudTypeVal);
    if (heightGradient <= 0.001) return 0.0;

    vec3 windOffset = vec3(uTime * 0.015 * uWindSpeed, 0.0, uTime * 0.008 * uWindSpeed);
    vec3 samplePos = normPos * 2.0 + windOffset;

    vec4 baseNoise = texture(uBaseNoise, samplePos);
    float lowFreqFBM = baseNoise.g * 0.625 + baseNoise.b * 0.25 + baseNoise.a * 0.125;
    float baseDensity = remap(baseNoise.r, lowFreqFBM - 1.0, 1.0, 0.0, 1.0);

    baseDensity *= heightGradient;

    float densityWithCoverage = remap(baseDensity, 1.0 - globalCoverage, 1.0, 0.0, 1.0);
    densityWithCoverage = clamp(densityWithCoverage, 0.0, 1.0);

    if (densityWithCoverage <= 0.001) return 0.0;

    vec3 detailPos = samplePos * 3.8 + windOffset * 0.5;
    vec3 detailNoise = texture(uDetailNoise, detailPos).rgb;
    float highFreqFBM = detailNoise.r * 0.625 + detailNoise.g * 0.25 + detailNoise.b * 0.125;

    float erosionModifier = mix(highFreqFBM, 1.0 - highFreqFBM, clamp(heightFraction * 2.0, 0.0, 1.0));
    float finalDensity = remap(densityWithCoverage, erosionModifier * uErosionScale * 0.55, 1.0, 0.0, 1.0);

    return clamp(finalDensity * uDensityScale, 0.0, 1.0);
}

// Light raymarching with multi-scattering. The step length is part of the
// optical depth; omitting it under-estimates sunlight extinction by the step
// length in metres.
float marchToSun(vec3 pos) {
    float stepLength = (uTopRadius - uBottomRadius) / max(uLightSteps, 1.0);
    vec3 lightStep = uSunDir * stepLength;
    float densityLength = 0.0;
    vec3 currPos = pos;

    for (uint i = 0u; i < MAX_LIGHT_STEPS; i++) {
        if (float(i) >= uLightSteps) break;
        currPos += lightStep;
        densityLength += sampleDensity(currPos) * stepLength;
    }

    float multiScatteringTransmittance = 0.0;
    float a = 0.5;
    float b = 0.5;

    for (uint oct = 0u; oct < MAX_MULTI_OCTAVES; oct++) {
        if (float(oct) >= uMultiScatteringOctaves) break;
        float powA = pow(a, float(oct));
        float powB = pow(b, float(oct));
        float opticalDepth = densityLength * uAbsorption * powA;
        multiScatteringTransmittance += exp(-opticalDepth) * powB;
    }

    return multiScatteringTransmittance;
}

// Per-step in-scatter (see 4.3). `stepTransmittance` is exp(-sigma_e * ds).
vec3 inscatter(vec3 samplePos, float density, float stepTransmittance) {
    float sigma_e = density * uAbsorption;
    float stepOpticalDepth = sigma_e * ds;
    float sunVisibility = marchToSun(samplePos);
    float phase = tripleLobePhase(cosTheta, uForwardG);
    float sunCos = saturate(dot(normalize(samplePos), uSunDir));
    float heightT = saturate((length(samplePos) - uBottomRadius) / layer);

    vec3 source = uSunColor * sunVisibility * phase
        + uSunColor * (0.99 / PI) * sunCos * mix(uGroundColor, vec3(1.0), heightT);
    return source * 0.99 * (1.0 - stepTransmittance);
}

// Atmospheric Background Calculation
vec3 calculateAtmosphericSky(vec3 rayDir) {
    float sunElevation = uSunDir.y;
    float sunDot = clamp(dot(rayDir, uSunDir), 0.0, 1.0);

    float horizonGradient = clamp(1.0 - abs(rayDir.y), 0.0, 1.0);
    vec3 zenSky = uSkyColor;
    vec3 horizSky = mix(uSkyColor, uGroundColor, 0.5) + vec3(0.15, 0.1, 0.05) * clamp(1.0 - sunElevation, 0.0, 1.0);

    vec3 skyColor = mix(zenSky, horizSky, pow(horizonGradient, 2.5));

    vec3 sunGlow = uSunColor * (pow(sunDot, 128.0) * 2.0 + pow(sunDot, 12.0) * 0.4);
    
    if (sunElevation < 0.3) {
        float sunsetFactor = clamp((0.3 - sunElevation) / 0.4, 0.0, 1.0);
        skyColor = mix(skyColor, vec3(0.9, 0.35, 0.15), sunsetFactor * pow(horizonGradient, 1.5));
    }

    return skyColor + sunGlow;
}

void main() {
    vec3 rayDir = normalize(vRayDirection);
    vec3 rayOrigin = uCameraPos;

    vec3 backgroundSky = calculateAtmosphericSky(rayDir);

    vec2 tBox = rayBoxIntersection(rayOrigin, rayDir, uBoxMin, uBoxMax);

    if (tBox.y <= 0.0 || tBox.x > tBox.y) {
        gl_FragColor = vec4(backgroundSky, 1.0);
        return;
    }

    float tNear = max(0.0, tBox.x);
    float tFar = tBox.y;

    float jitter = texture2D(uBlueNoise, gl_FragCoord.xy / 64.0).r;
    float stepSize = (tFar - tNear) / float(uPrimarySteps);
    float currentDist = tNear + stepSize * jitter;

    float cosTheta = dot(rayDir, uSunDir);
    float phaseValue = tripleLobePhase(cosTheta, uForwardG);

    float accumulatedTransmittance = 1.0;
    vec3 accumulatedColor = vec3(0.0);

    for (int i = 0; i < 128; i++) {
        if (i >= uPrimarySteps || currentDist > tFar || accumulatedTransmittance < 0.005) break;

        vec3 samplePos = rayOrigin + rayDir * currentDist;
        float density = sampleDensity(samplePos);

        if (density > 0.001) {
            float stepAbsorb = density * uAbsorption * stepSize * 0.22;
            float stepTransmittance = exp(-stepAbsorb);

            float sunTransmittance = marchToSun(samplePos, cosTheta);
            vec3 directLight = uSunColor * sunTransmittance * phaseValue * 2.8;

            float heightFrac = (samplePos.y - uBoxMin.y) / (uBoxMax.y - uBoxMin.y);
            vec3 ambientLight = mix(uGroundColor * 0.35, uSkyColor * 0.85, heightFrac);

            vec3 stepScattering = (directLight + ambientLight) * density;

            accumulatedColor += accumulatedTransmittance * (stepScattering - stepScattering * stepTransmittance) / max(stepAbsorb, 0.0001);
            accumulatedTransmittance *= stepTransmittance;
            
            currentDist += stepSize;
        } else {
            currentDist += stepSize * 1.8;
        }
    }

    // Premultiplied composite; aerial perspective is applied by the shared
    // atmospheric fog, so no haze blend lives in this shader.
    vec3 finalColor = accumulatedColor
        + backgroundSky * (1.0 - accumulatedTransmittance);
    
    // ACES Filmic Tone Mapping
    finalColor = clamp((finalColor * (2.51 * finalColor + 0.03)) / (finalColor * (2.43 * finalColor + 0.59) + 0.14), 0.0, 1.0);
    finalColor = pow(finalColor, vec3(1.0 / 2.2));

    gl_FragColor = vec4(finalColor, 1.0);
}
```

---

## 7. Performance Optimizations & Adaptive Sampling Strategies

1. **Quarter-Resolution Rendering with Temporal Reprojection:**
   * Render cloud volume ray marching into a render target at half or quarter resolution ($50\% \times 50\%$).
   * Upscale to full resolution using a 4x4 Bilateral Upsampling Filter that checks camera depth discontinuities.
   * Apply Temporal Anti-Aliasing (TAA) reprojection using motion vectors to smooth out dithering jitter.

2. **Adaptive Primary Step Scaling:**
   * Scale `uPrimarySteps` dynamically based on frame timing. If target frame rate drops below $45\,\text{FPS}$, drop steps from $64 \to 48 \to 32$.
   * Increase step sizes when camera distance to volume shell $t_{\text{entry}} > 100\,\text{m}$.

3. **Precomputed 3D Noise Channels:**
   * Store 3D Perlin-Worley base noise in an $8\text{-bit}$ RGBA 3D texture ($64 \times 64 \times 64 = 1\,\text{MB}$).
   * Store detail Worley noise in a $32 \times 32 \times 32$ 3D texture ($128\,\text{KB}$).

---

## 8. Weather Map Specification

The 2D Weather Map RGBA texture drives real-time atmospheric morphometry:

* **Red Channel ($R$):** Cloud Coverage ($0.0 = \text{Clear Sky}$, $1.0 = \text{Overcast}$).
* **Green Channel ($G$):** Precipitation / Density Scale ($0.0 = \text{Light Wisps}$, $1.0 = \text{Heavy Rain Volume}$).
* **Blue Channel ($B$):** Cloud Type Altitude Morph ($0.0 = \text{Stratus}$, $0.5 = \text{Stratocumulus}$, $1.0 = \text{Cumulonimbus}$).
* **Alpha Channel ($A$):** Wind Drift / Directional Mask.

---

## 9. Preset Parameter Matrix

| Preset Name | Coverage ($R$) | Cloud Type ($B$) | Density Scale | Absorption ($\sigma_e$) | Sun Elevation | $g_1$ Forward |
| ----- | ----- | ----- | ----- | ----- | ----- | ----- |
| **Clear Day** | $0.58$ | $0.68$ | $0.85$ | $0.32$ | $+32^\circ$ | $0.82$ |
| **Golden Hour** | $0.65$ | $0.82$ | $1.15$ | $0.45$ | $+3^\circ$ | $0.89$ |
| **Overcast Storm**| $0.90$ | $0.95$ | $2.10$ | $0.85$ | $+15^\circ$ | $0.55$ |
| **Altocumulus** | $0.38$ | $0.12$ | $0.42$ | $0.20$ | $+52^\circ$ | $0.84$ |
| **Moonlit Night** | $0.52$ | $0.60$ | $0.90$ | $0.30$ | $+25^\circ$ | $0.75$ |
