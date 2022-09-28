use ::lazy_static::lazy_static;
use std::env;

lazy_static! {
    static ref TARGET_FEATURES: Vec<String> = env::var("CARGO_CFG_TARGET_FEATURE")
        .expect("There are no target features! Something is very wrong!")
        .split(',')
        .map(str::to_owned)
        .collect();
}

fn main() {
    match target_arch().as_str() {
        "x86_64" => {
            translate_simd_features!(
                simd-x4 => ssse3 ? SSSE3
                simd-m4 => avxvnni ? AVX_VNNI_HALF

                simd-x8 => avx ? AVX
                simd-m8 => avxvnni ? AVX_VNNI

                simd-x16 => avx512bw ? AVX_512
                simd-m16 => avx512vnni ? AVX_512_VNNI
            );
        }
        "aarch64" => {
            translate_simd_features!(
                simd-x4 => neon ? NEON
                simd-m4 => neon ? NEON

                simd-x8 => sve ? SVE
                simd-m8 => sve ? SVE

                simd-x16 => sve2 ? SVE2
                simd-m16 => sve2 ? SVE2
            );
        }
        _ => { /*to be added later*/ }
    };
}

fn feature_enabled(feature: &String) -> bool {
    let key = "CARGO_FEATURE_".to_owned() + &feature.to_uppercase().replace('-', "_");
    env::var(key).is_ok()
}

fn target_feature_enabled(target_feature: &String) -> bool {
    TARGET_FEATURES.contains(target_feature)
}

fn target_arch() -> String {
    env::var("CARGO_CFG_TARGET_ARCH").unwrap()
}

#[macro_export]
macro_rules! translate_simd_features {
    ($($feature:stmt => $target_feature:ident ? $option:ident)*) => {
        let mut levels: Vec<(String, String, String)> = Vec::with_capacity(6);

        $(
            levels.push(
                (
                    stringify!($feature).replace(' ', "").replace(';', ""),
                    String::from(stringify!($target_feature)),
                    String::from(stringify!($option))
                )
            );
        )+
        levels.reverse();

        if feature_enabled(&String::from("simd-max")) {
            let mut feat_detected = false;
            for (ft, tgt_ft, opt) in levels {
                if target_feature_enabled(&tgt_ft) {
                    println!("hardqoi: selecting {ft} as the best SIMD feature");
                    println!("cargo:rustc-cfg={}", opt);
                    feat_detected = true;
                    break;
                }
            }
            if !feat_detected {
                if !feature_enabled(&String::from("shut-up")) {
                    println!("cargo:warning=No vectorized hardqoi implementations could be enabled!")
                }
                println!("cargo:rustc-cfg=NONE");
            }
        } else {


            let shut_up = feature_enabled(&String::from("shut-up"));
            let force_compile = feature_enabled(&String::from("force-compile"));

            let mut feat_detected = false;
            for (feat, tgt_feat, opt) in levels {
                if feature_enabled(&feat) {
                    if target_feature_enabled(&tgt_feat) {
                        println!("cargo:rustc-cfg={opt}");
                        println!("hardqoi: compiling requested {opt} method");
                        feat_detected = true;
                        break;
                    } else if force_compile {
                        println!("cargo:rustc-cfg={opt}");
                        println!("hardqoi: forcing compilation of {opt} without {tgt_feat} support");
                        if !shut_up {
                            println!("cargo:warning=\
                                Forcing compilation of hardqoi with {f} without compiling machine hardware support for {t}.
                                If it is run on hardware without the required features then the library fail to execute!
                                Enable -C target-cpu=native if you know your hardware can do {f} and have not already.
                                Use the simd-max feature for the fastest version of hardqoi for your machine.\n\
                            ", f = feat, t = tgt_feat);
                        }
                        feat_detected = true;
                        break;
                    } else if !shut_up {
                        // since it was requested, we should warn that it is not available
                        println!("cargo:warning=\
                            The feature {f} requires the target feature {t} which does not appear to be enabled on your platform! \
                            The library will be compiled without these features so that it will run correctly.\n\
                            Enable -C target-cpu=native and the simd-max feature for the fastest version of hardqoi for your machine.\n\
                            Otherwise, use the force-compile, shut-up, and simd-x/mX features to compile what you want.
                        ", f = feat, t = tgt_feat);
                    }
                }
            }
            if !feat_detected {
                if !feature_enabled(&String::from("shut-up")) {
                    println!("cargo:warning=No vectorized hardqoi implementations could be enabled!")
                }
                println!("cargo:rustc-cfg=NONE");
            }
        }
    };
}
