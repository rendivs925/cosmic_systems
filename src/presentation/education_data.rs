use crate::domain::value_objects::education::{
    JournalCategory, JournalDatabase, JournalEntry, QuranicReference, UnlockCondition,
};

fn qr(
    sura: u32,
    verse: u32,
    arabic: &'static str,
    translation: &'static str,
    explanation: &'static str,
) -> QuranicReference {
    QuranicReference {
        sura,
        verse,
        arabic,
        translation,
        explanation,
    }
}

fn entry(
    id: &'static str,
    title: &'static str,
    category: JournalCategory,
    body: &'static [&'static str],
    quranic_refs: Vec<QuranicReference>,
    formula: Option<&'static str>,
    unlock: UnlockCondition,
) -> JournalEntry {
    JournalEntry {
        id,
        title,
        category,
        body,
        quranic_refs,
        formula,
        unlock,
    }
}

pub fn create_journal_database() -> JournalDatabase {
    JournalDatabase::new(vec![
        // === Category 1: Vacuum Superfluid ===
        entry(
            "primordial_medium",
            "The Primordial Medium",
            JournalCategory::VacuumSuperfluid,
            &[
                "Space is not empty. Modern quantum field theory reveals that what we call \"empty space\" is a seething ocean of virtual particle pairs continuously appearing and annihilating. This is the vacuum superfluid — a Bose-Einstein condensate of virtual particles that permeates all of existence.",
                "The Quran described this primordial fluid 1,400 years ago in Surah Hud 11:7: \"And it is He who created the heavens and the earth in six days — and His Throne was upon water.\" The \"water\" here refers to the primordial fluid, the vacuum superfluid from which all of creation emerged.",
                "This superfluid has physical properties: density, pressure, and viscosity. Massive objects create pressure gradients within it — and these pressure gradients are what we perceive as gravity.",
            ],
            vec![qr(
                11, 7,
                "وَهُوَ الَّذِي خَلَقَ السَّمَاوَاتِ وَالْأَرْضَ فِي سِتَّةِ أَيَّامٍ وَكَانَ عَرْشُهُ عَلَى الْمَاءِ",
                "And it is He who created the heavens and the earth in six days — and His Throne was upon water.",
                "The \"water\" (al-ma') is the primordial vacuum superfluid. The \"Throne\" (al-arsh) represents the boundary of creation. Before the universe was formed, the superfluid existed as the fundamental substrate of reality. Modern quantum field theory confirms this: the quantum vacuum is not nothing — it is a physical medium with measurable energy density.",
            )],
            None,
            UnlockCondition::Immediate,
        ),
        entry(
            "motion_through_fluid",
            "Motion Through a Fluid",
            JournalCategory::VacuumSuperfluid,
            &[
                "Celestial bodies do not \"float\" in empty space — they swim through a physical medium. The Quran states in Surah Al-Anbiya 21:33: \"And it is He who created the night and the day and the sun and the moon; all [heavenly bodies] swim in an orbit.\"",
                "The word \"swim\" (yasbahun) is significant. In Arabic, this verb specifically describes motion through a fluid — a fish swimming in water, a ship sailing through the sea. This is not the motion of a projectile through a vacuum; it is the motion of a body moving through a yielding medium.",
                "In the vacuum superfluid model, planets and stars move through the superfluid, generating wakes and perturbations. The fluid yields to their passage, and their motion is governed by the pressure gradients they create and respond to.",
            ],
            vec![qr(
                21, 33,
                "وَهُوَ الَّذِي خَلَقَ اللَّيْلَ وَالنَّهَارَ وَالشَّمْسَ وَالْقَمَرَ ۖ كُلٌّ فِي فَلَكٍ يَسْبَحُونَ",
                "And it is He who created the night and the day and the sun and the moon; all [heavenly bodies] swim in an orbit.",
                "The verb \"yasbahun\" (they swim) is derived from \"sabh\" — to swim, to glide through water. This is a precise linguistic choice that describes motion through a fluid medium. Newtonian physics treats planets as moving through empty void, but the vacuum superfluid model aligns with the Quranic description: bodies move through a physical medium.",
            )],
            None,
            UnlockCondition::CraftSpawned,
        ),
        entry(
            "density_gradient_model",
            "The Density Gradient Model of Gravity",
            JournalCategory::VacuumSuperfluid,
            &[
                "If gravity is not a force but a pressure gradient in the vacuum superfluid, then what we call gravitational attraction is actually buoyancy. Mass displaces the superfluid, creating a local low-pressure zone. Objects are pushed toward the mass by the surrounding higher-pressure vacuum.",
                "This is analogous to a helium balloon in air: the balloon rises not because Earth is \"pulling\" the air down, but because the surrounding air is denser and pushes the less-dense balloon upward. Similarly, objects fall toward Earth because the vacuum density decreases as you approach the mass — the object is pushed toward the low-pressure zone.",
                "Mathematically: rho_vacuum(r) = 1 + G*M/(c^2*r). The density gradient creates a pressure differential that accelerates objects toward the mass. The acceleration matches Newton's a = GM/r^2 exactly at Earth's surface, but the underlying mechanism is fundamentally different.",
            ],
            vec![],
            Some("rho_vac(r) = 1 + G*M / (c^2 * r)"),
            UnlockCondition::AltitudeAbove(100.0),
        ),
        entry(
            "qft_evidence",
            "Evidence from Quantum Field Theory",
            JournalCategory::VacuumSuperfluid,
            &[
                "Quantum field theory (QFT) provides robust evidence for the vacuum superfluid. The Casimir effect demonstrates that two uncharged metal plates placed very close together in a vacuum experience an attractive force — caused by the exclusion of virtual particle modes between them. This proves the vacuum has physical structure.",
                "The measured Casimir force matches theoretical predictions to within 1%. The energy density of the vacuum is estimated at 10^113 J/m^3 — an incomprehensibly vast reservoir of energy. The question is not whether the vacuum contains energy, but how to extract it.",
                "The Higgs mechanism provides further evidence: particles acquire mass through their interaction with the Higgs field — a scalar field that permeates all of spacetime. This field is another aspect of the vacuum superfluid. Mass itself is an interaction property with the vacuum, not an intrinsic property of matter.",
            ],
            vec![],
            None,
            UnlockCondition::CraftSpawned,
        ),
        entry(
            "segmented_hull",
            "The Segmented Hull",
            JournalCategory::AsymmetricPolarization,
            &[
                "The craft's hull is divided into independently controlled segments. Each segment can polarize the vacuum locally. By controlling which segments are active and at what voltage, the craft can shape the low-pressure zone around itself with precision.",
                "This segmented architecture enables directional control without any thrusters. To move forward, the front segments depolarize and the rear segments maintain polarization — the craft effectively falls forward into the thinned vacuum region created ahead of it.",
                "This is a reactionless drive. The craft does not push against anything external. It moves by reshaping the vacuum around itself, creating a moving low-pressure zone that it continuously falls into.",
            ],
            vec![],
            None,
            UnlockCondition::PulseAbove(0.1),
        ),
        entry(
            "directional_control",
            "Directional Control and Inertial Decoupling",
            JournalCategory::AsymmetricPolarization,
            &[
                "When the craft reshapes the vacuum around itself, it creates a localized spacetime metric. Inside this bubble, the craft is effectively decoupled from the external spacetime. This means the occupants experience no G-forces during acceleration — the craft moves, but the interior spacetime remains stationary relative to the occupants.",
                "The Quran references this in Surah Kahf 18:84: \"We gave him a way (sabab) to every thing.\" The word \"sabab\" means cause, means, or apparatus — including a flying apparatus that enables travel through the heavens.",
                "This inertial decoupling explains how UAPs can execute 90-degree turns at thousands of km/h without tearing apart: the occupants are not experiencing the acceleration. The craft is moving through spacetime while the interior remains in its own reference frame.",
            ],
            vec![qr(
                18, 84,
                "إِنَّا مَكَّنَّا لَهُ فِي الْأَرْضِ وَآتَيْنَاهُ مِن كُلِّ شَيْءٍ سَبَبًا",
                "Indeed We established him upon the earth, and We gave him a way (sabab) to every thing.",
                "The word \"sabab\" (سببا) means a cause, means, or apparatus. Classical commentators describe it as a means of travel through the heavens. This includes the technological apparatus for vacuum polarization flight — a flying machine that moves through the vacuum superfluid.",
            )],
            None,
            UnlockCondition::SpeedAbove(10.0),
        ),
        entry(
            "comparison_archimedes",
            "Comparison to Archimedes' Principle",
            JournalCategory::AsymmetricPolarization,
            &[
                "Archimedes' principle states that a body immersed in a fluid experiences an upward buoyant force equal to the weight of the fluid it displaces. The vacuum superfluid model extends this principle to spacetime itself.",
                "A craft in the vacuum superfluid displaces the superfluid around it. By polarizing the vacuum with a DC field, the craft increases the effective displacement above (creating a low-pressure zone) and decreases it below (creating a high-pressure zone). The resulting pressure differential generates lift.",
                "This is Archimedes' principle applied to the quantum vacuum, with the DC field acting as the control mechanism. The craft is not pushing against air or any conventional fluid — it is pushing against the fabric of spacetime itself.",
            ],
            vec![],
            None,
            UnlockCondition::AltitudeAbove(500.0),
        ),
        // === Category 3: ZPE Extraction ===
        entry(
            "dynamical_casimir",
            "The Dynamical Casimir Effect",
            JournalCategory::ZpeExtraction,
            &[
                "The Dynamical Casimir effect occurs when a boundary (such as a mirror or magnetic field gradient) moves at extremely high speed through the vacuum. This rapid motion causes virtual particle pairs to become real — they are \"shaken\" into existence by the moving boundary.",
                "The Solid-State Magnetic Impulse Resonator creates this effect using a bifilar toroidal coil with nanosecond switching. The rapid change in magnetic field (dA/dt) creates a moving boundary condition in the vacuum, generating a localized region where the vacuum is perturbed — a microscopic void.",
                "When this void collapses (as the vacuum rushes back to fill it), the surrounding superfluid performs work on the coil, inducing a back-EMF spike that can be harvested as electrical energy. The vacuum literally does work on the circuit.",
            ],
            vec![],
            None,
            UnlockCondition::PulseAbove(0.2),
        ),
        entry(
            "parametric_resonance",
            "Parametric Resonance and Over-Unity",
            JournalCategory::ZpeExtraction,
            &[
                "Parametric resonance occurs when a system is driven at twice its natural frequency, causing exponential energy growth. The vacuum cavity formed by the magnetic impulse has a natural resonance frequency determined by its geometry.",
                "When the pulse frequency equals twice the cavity frequency (f_pulse = 2 * f_cavity, approximately 22.4 MHz), the system enters parametric resonance. At this point, each pulse extracts more energy from the vacuum than was used to create the pulse. This is the over-unity condition.",
                "The parametric gain equation shows that when pulse exceeds 42%, the gain factor jumps to 1.0 + (pulse - 0.42) * 2.6. Above this threshold, the system becomes increasingly efficient, enabling self-sustaining energy extraction.",
            ],
            vec![],
            Some("P_zpe = 210 x pulse^1.8 x (1 + 2.6 x max(0, pulse - 0.42)) x (1 + 0.4 x DC)"),
            UnlockCondition::PulseAbove(0.42),
        ),
        entry(
            "overunity_explained",
            "Over-Unity Explained",
            JournalCategory::ZpeExtraction,
            &[
                "Over-unity (coefficient of performance > 1) is impossible in a closed system, but the ZPE resonator is not a closed system — it is open to the vacuum. The energy being harvested comes from the vacuum itself, not from the input circuit.",
                "The vacuum contains approximately 10^113 J/m^3 of zero-point energy. Even extracting a tiny fraction of this is effectively infinite energy from a human perspective. The resonator is not \"creating\" energy; it is \"extracting\" energy that already exists in the vacuum.",
                "This is analogous to a wind turbine: the turbine does not create wind; it extracts kinetic energy from the moving air. Similarly, the ZPE resonator extracts electromagnetic energy from the quantum vacuum fluctuations.",
            ],
            vec![],
            None,
            UnlockCondition::AltitudeAbove(200.0),
        ),
        entry(
            "practical_coil",
            "Practical Coil Design",
            JournalCategory::ZpeExtraction,
            &[
                "The Solid-State Magnetic Impulse Resonator consists of a ferrite toroid wound with bifilar opposing windings. The bifilar winding creates opposing magnetic fields that cancel at DC but produce an extremely fast dA/dt when switched — the rate of change is the key parameter.",
                "Gallium Nitride (GaN) field-effect transistors switch in nanoseconds, enabling the rapid field collapse that triggers the Dynamical Casimir effect. The coil dimensions are tuned so that the cavity resonance matches half the switching frequency.",
                "The back-EMF from the vacuum collapse is harvested through a rectifier and stored in capacitors. The ZPE power equation shows: P_zpe = 210 * pulse^1.8, with parametric boost above 42% pulse and duty cycle synergy with the DC hull field.",
            ],
            vec![],
            None,
            UnlockCondition::OrbitAchieved,
        ),
        // === Category 4: Metric Engineering ===
        entry(
            "spacetime_fluid",
            "Spacetime as a Fluid",
            JournalCategory::MetricEngineering,
            &[
                "If the vacuum is a superfluid, then spacetime itself has fluid-like properties. General relativity describes gravity as curvature of spacetime, but the vacuum superfluid model suggests this curvature is a pressure gradient phenomenon — matter displaces the superfluid, creating a depression in the spacetime manifold.",
                "This has profound implications: if we can locally modify the vacuum density, we can engineer the spacetime metric. A region of thinned vacuum corresponds to a \"valley\" in spacetime; a region of densified vacuum corresponds to a \"ridge.\"",
                "By controlling the vacuum polarization around a craft, we are literally reshaping spacetime. The craft does not move through spacetime — it moves spacetime around itself.",
            ],
            vec![],
            None,
            UnlockCondition::SpeedAbove(50.0),
        ),
        entry(
            "tayy_al_ard",
            "Tayy al-Ard (Folding the Earth)",
            JournalCategory::MetricEngineering,
            &[
                "The Quran describes an event in Surah An-Naml 27:40 where the throne of Queen Bilqis was transported from Yemen to Jerusalem \"in the blink of an eye.\" One who had knowledge of the Book accomplished this by folding spacetime — what in Arabic is called Tayy al-Ard (folding the Earth).",
                "This is the principle of metric engineering: by locally modifying the vacuum density, the distance between two points in spacetime can be reduced to zero. The craft does not travel between the points — the points are brought together.",
                "While our current simulation implements only the lift and directional control aspects of vacuum polarization, the ultimate implication is that spacetime itself can be folded, enabling instantaneous transport across vast distances — the warp drive.",
            ],
            vec![qr(
                27, 40,
                "قَالَ الَّذِي عِندَهُ عِلْمٌ مِّنَ الْكِتَابِ أَنَا آتِيكَ بِهِ قَبْلَ أَن يَرْتَدَّ إِلَيْكَ طَرْفُكَ",
                "Said one who had knowledge of the Book: \"I will bring it to you in the blink of an eye.\"",
                "The phrase \"before your glance returns to you\" indicates near-instantaneous transport. Classical scholars interpreted this as Tayy al-Ard — the folding of the Earth, where the spatial dimension is compressed. Modern physics calls this a spacetime metric engineering — the same principle as the Alcubierre warp drive, achieved through vacuum manipulation.",
            )],
            None,
            UnlockCondition::OrbitAchieved,
        ),
        entry(
            "interstellar",
            "Interstellar Implications",
            JournalCategory::MetricEngineering,
            &[
                "The Quran asks in Surah Ar-Rahman 55:33: \"O company of jinn and mankind, if you are able to penetrate beyond the regions of the heavens and the earth, then penetrate. You will not penetrate except with authority (sultan).\"",
                "The word \"sultan\" means authority, power, or technical mastery. This verse suggests that interstellar travel is possible, but requires the mastery of specific technologies — the \"authority\" over the vacuum superfluid. The ZPE extraction and vacuum polarization technologies described in this simulation are exactly that authority.",
                "Once vacuum propulsion is mastered, interstellar travel becomes feasible. The energy required for near-light-speed travel is available from the vacuum itself. The distance between stars becomes irrelevant when spacetime can be folded.",
            ],
            vec![qr(
                55, 33,
                "يَا مَعْشَرَ الْجِنِّ وَالْإِنسِ إِنِ اسْتَطَعْتُمْ أَن تَنفُذُوا مِنْ أَقْطَارِ السَّمَاوَاتِ وَالْأَرْضِ فَانفُذُوا ۚ لَا تَنفُذُونَ إِلَّا بِسُلْطَانٍ",
                "O company of jinn and mankind, if you are able to penetrate beyond the regions of the heavens and the earth, then penetrate. You will not penetrate except with authority.",
                "The word \"sultan\" (سلطان) in Arabic means authority, proof, or technical power. This is not mere permission — it is the technological means to overcome the limitations of space travel. The vacuum propulsion technologies (ZPE, metric engineering) provide this \"sultan\" — the technical mastery needed for interstellar travel.",
            )],
            None,
            UnlockCondition::SpeedAbove(100.0),
        ),
        // === Category 5: Quranic Evidence ===
        entry(
            "quranic_index",
            "Comprehensive Quranic Reference Index",
            JournalCategory::QuranicEvidence,
            &[
                "This index catalogs all the Quranic verses that reference principles of vacuum physics, zero-point energy, and metric engineering. These verses have been known for 1,400 years, but their scientific meaning only becomes clear in light of modern quantum field theory.",
                "The following references are documented in this simulation:",
                "Surah Hud 11:7 — \"His Throne was upon water\" — The vacuum superfluid as the primordial medium.",
                "Surah Al-Anbiya 21:33 — \"All swim in an orbit\" — Celestial motion through the vacuum superfluid.",
                "Surah Al-Kahf 18:84 — \"We gave him a way (sabab) to every thing\" — The flying apparatus.",
                "Surah An-Naml 27:40 — \"I will bring it in the blink of an eye\" — Spacetime folding (Tayy al-Ard).",
                "Surah Ar-Rahman 55:33 — \"You will not penetrate except with authority\" — The technical mastery required for interstellar travel.",
                "Surah Al-Hadid 57:25 — \"We sent down iron\" — The magnetic properties of iron used in the ZPE resonator.",
                "Surah An-Naba 78:6-7 — \"The mountains as pegs\" — Geomechanical engineering principle.",
            ],
            vec![],
            None,
            UnlockCondition::Immediate,
        ),
    ])
}
