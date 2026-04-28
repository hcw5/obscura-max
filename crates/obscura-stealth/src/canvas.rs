use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserProfile {
    pub profile_id: String,
    pub webgl: WebGlProfile,
    #[serde(default)]
    pub canvas_seed: Option<u64>,
    #[serde(default)]
    pub offline_audio_seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebGlProfile {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CanvasBindings {
    pub canvas_seed: u64,
    pub offline_audio_seed: u64,
    pub bootstrap_js: String,
}

impl CanvasBindings {
    pub fn from_profile(profile: &BrowserProfile) -> Self {
        let canvas_seed = profile
            .canvas_seed
            .unwrap_or_else(|| deterministic_seed(&profile.profile_id, "canvas"));
        let offline_audio_seed = profile
            .offline_audio_seed
            .unwrap_or_else(|| deterministic_seed(&profile.profile_id, "offline-audio"));

        let profile_json = serde_json::json!({
            "canvasSeed": canvas_seed,
            "offlineAudioSeed": offline_audio_seed,
            "webgl": {
                "vendor": profile.webgl.vendor,
                "renderer": profile.webgl.renderer,
                "version": profile.webgl.version,
                "extensions": profile.webgl.extensions,
            }
        });

        Self {
            canvas_seed,
            offline_audio_seed,
            bootstrap_js: format!(
                "globalThis.__obscura_profile = Object.freeze({});\n{}",
                profile_json,
                js_bindings()
            ),
        }
    }
}

fn deterministic_seed(profile_id: &str, scope: &str) -> u64 {
    // Stable across sessions for same profile_id, different across ids/scopes.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in profile_id
        .as_bytes()
        .iter()
        .chain(std::iter::once(&0xff))
        .chain(scope.as_bytes().iter())
    {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn js_bindings() -> &'static str {
    r#"(function installObscuraStealthBindings() {
  const profile = globalThis.__obscura_profile;
  if (!profile || !profile.webgl) return;

  function seededNoise(seed, index) {
    let h = (seed ^ (index * 0x9E3779B1)) >>> 0;
    h ^= h >>> 16;
    h = Math.imul(h, 0x7feb352d);
    h ^= h >>> 15;
    h = Math.imul(h, 0x846ca68b);
    h ^= h >>> 16;
    return h & 0xff;
  }

  const canvasToDataURL = HTMLCanvasElement.prototype.toDataURL;
  HTMLCanvasElement.prototype.toDataURL = function(...args) {
    const base = canvasToDataURL.apply(this, args);
    const suffix = (profile.canvasSeed >>> 0).toString(16).padStart(8, '0');
    return `${base}#${suffix}`;
  };

  const getImageData = CanvasRenderingContext2D.prototype.getImageData;
  CanvasRenderingContext2D.prototype.getImageData = function(sx, sy, sw, sh) {
    const imageData = getImageData.call(this, sx, sy, sw, sh);
    const data = imageData.data;
    const baseSeed = profile.canvasSeed >>> 0;
    for (let i = 0; i < data.length; i += 4) {
      data[i] = (data[i] + (seededNoise(baseSeed, i) & 0x03)) & 0xff;
      data[i + 1] = (data[i + 1] + (seededNoise(baseSeed, i + 1) & 0x03)) & 0xff;
      data[i + 2] = (data[i + 2] + (seededNoise(baseSeed, i + 2) & 0x03)) & 0xff;
    }
    return imageData;
  };

  const startRendering = OfflineAudioContext.prototype.startRendering;
  OfflineAudioContext.prototype.startRendering = function(...args) {
    return Promise.resolve(startRendering.apply(this, args)).then((buffer) => {
      const seed = profile.offlineAudioSeed >>> 0;
      for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
        const data = buffer.getChannelData(ch);
        for (let i = 0; i < data.length; i += 64) {
          const n = (seededNoise(seed, (ch << 20) + i) / 255 - 0.5) * 1e-7;
          data[i] += n;
        }
      }
      return buffer;
    });
  };

  const kDebug = 0x9245;
  const kRenderer = 0x9246;
  const kVersion = 0x1F02;

  function patchWebGL(ctor) {
    if (!ctor || !ctor.prototype) return;
    const proto = ctor.prototype;
    const originalGetParameter = proto.getParameter;
    const originalGetSupportedExtensions = proto.getSupportedExtensions;
    const originalGetExtension = proto.getExtension;

    proto.getParameter = function(pname) {
      if (pname === kDebug) return profile.webgl.vendor;
      if (pname === kRenderer) return profile.webgl.renderer;
      if (pname === kVersion) return profile.webgl.version;
      return originalGetParameter.call(this, pname);
    };

    proto.getSupportedExtensions = function() {
      return profile.webgl.extensions.slice();
    };

    proto.getExtension = function(name) {
      if (!profile.webgl.extensions.includes(name)) return null;
      return originalGetExtension.call(this, name);
    };
  }

  patchWebGL(globalThis.WebGLRenderingContext);
  patchWebGL(globalThis.WebGL2RenderingContext);
})();"#
}

#[cfg(test)]
mod tests {
    use super::{BrowserProfile, CanvasBindings, WebGlProfile};

    fn profile(id: &str) -> BrowserProfile {
        BrowserProfile {
            profile_id: id.to_string(),
            webgl: WebGlProfile {
                vendor: "Google Inc. (NVIDIA)".to_string(),
                renderer: "ANGLE (NVIDIA)".to_string(),
                version: "WebGL 2.0".to_string(),
                extensions: vec!["WEBGL_debug_renderer_info".to_string()],
            },
            canvas_seed: None,
            offline_audio_seed: None,
        }
    }

    #[test]
    fn seeds_are_stable_for_same_profile() {
        let a = CanvasBindings::from_profile(&profile("profile-a"));
        let b = CanvasBindings::from_profile(&profile("profile-a"));
        assert_eq!(a.canvas_seed, b.canvas_seed);
        assert_eq!(a.offline_audio_seed, b.offline_audio_seed);
    }

    #[test]
    fn seeds_vary_between_profiles() {
        let a = CanvasBindings::from_profile(&profile("profile-a"));
        let b = CanvasBindings::from_profile(&profile("profile-b"));
        assert_ne!(a.canvas_seed, b.canvas_seed);
        assert_ne!(a.offline_audio_seed, b.offline_audio_seed);
    }
}
