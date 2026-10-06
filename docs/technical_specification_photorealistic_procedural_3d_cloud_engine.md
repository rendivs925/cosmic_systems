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
| **Light Transport** | Single Lambertian or Blinn-Phong directional shading applied to density. | **Beer-Lambert Extinction** + **Dual-Lobe Henyey-Greenstein Phase Function** + **Powder Effect** + **Multiple Scattering Approximation**. | 
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

### 3.4 Wind Dynamics & Curl-Noise Advection

Advect cloud structures over time $t$ along wind vector $\vec{V}_{\text{wind}}$ with a continuous 3D velocity field derived from **Curl Noise** to prevent uniform sliding:

$$
\vec{p}_{\text{sampled}} = \vec{p} + t \cdot \vec{V}_{\text{wind}} + \left( \nabla \times \vec{\Psi}(\vec{p}) \right) \cdot A_{\text{turb}}
$$

Where $\vec{\Psi}$ is a 3D potential field composed of simplex noise, guaranteeing mass-conserving, divergence-free incompressible flow ($\nabla \cdot \vec{v} = 0$).

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

### 4.3 Powder Sugar Effect (Dark Edge Inversion)

Thick cumulus cloud tops viewed toward the light source exhibit high internal reflection. However, deep inside clouds, light depletes rapidly. To prevent deep interior clouds from shading into pure flat black, apply the **Sugar/Powder Effect**:

$$
E_{\text{powder}}(\rho, \Delta s) = 1.0 - \exp\left( -2.0 \cdot \rho \cdot \Delta s \right)
$$

$$
T_{\text{phase}}(\theta, \rho, \Delta s) = 2.0 \cdot P_{\text{triple}}(\theta) \cdot E_{\text{powder}}(\rho, \Delta s)
$$

### 4.4 Multiple Scattering Approximation (Octave-Based Attenuation Decay)

Single-scattering models render cloud centers too dark. To simulate multiple internal bounces without expensive unbiased path tracing, accumulate scattering across $K$ attenuation octaves with decreasing density scale and increasing forward scattering anisotropy:

$$
T_{\text{multi}}(\rho, d_{\text{sun}}) = \sum_{k=0}^{K-1} \frac{c_k}{2^k} \cdot \exp\left( -d_{\text{sun}} \cdot \sigma_e \cdot a^k \cdot \rho \right)
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

The following complete fragment shader implements volumetric ray marching, 3D Perlin-Worley density evaluation, multi-octave light transport, phase functions, and atmospheric fog blending.

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
uniform float uPowderStrength;
uniform float uWindSpeed;
uniform float uAerialHaze;
uniform float uMultiScatteringOctaves;

uniform int uPrimarySteps;
uniform int uLightSteps;

uniform sampler3D uBaseNoise;
uniform sampler3D uDetailNoise;
uniform sampler2D uWeatherMap;
uniform sampler2D uBlueNoise;

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

// Light Raymarching with Multi-Scattering
float marchToSun(vec3 pos, float cosTheta) {
    vec3 lightStep = uSunDir * ((uBoxMax.y - uBoxMin.y) / float(uLightSteps) * 0.12);
    float totalDensity = 0.0;
    vec3 currPos = pos;

    for (int i = 0; i < 6; i++) {
        currPos += lightStep;
        totalDensity += sampleDensity(currPos);
    }

    // Multi-scattering octaves
    float multiScatteringTransmittance = 0.0;
    float a = 0.5;
    float b = 0.5;

    for (float oct = 0.0; oct < 4.0; oct += 1.0) {
        if (oct >= uMultiScatteringOctaves) break;
        
        float powA = pow(a, oct);
        float powB = pow(b, oct);

        float opticalDepth = totalDensity * uAbsorption * powA;
        float stepTransmittance = exp(-opticalDepth);
        
        multiScatteringTransmittance += stepTransmittance * powB;
    }

    // Powder sugar edge darkener
    float powder = 1.0 - exp(-totalDensity * uAbsorption * 2.5 * uPowderStrength);
    float powderBlend = mix(1.0, powder, clamp(cosTheta * 0.5 + 0.5, 0.0, 1.0));

    return multiScatteringTransmittance * powderBlend;
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

    float distanceFog = clamp((tNear / 180.0) * uAerialHaze, 0.0, 0.85);
    vec3 cloudResult = mix(accumulatedColor, backgroundSky * (1.0 - accumulatedTransmittance), distanceFog);

    vec3 finalColor = mix(backgroundSky, cloudResult, 1.0 - accumulatedTransmittance);
    
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
