use super::*;

fn status(anon: u64, file: u64, shmem: u64) -> ProcStatusMemoryFields {
    ProcStatusMemoryFields {
        rss_bytes: anon + file + shmem,
        rss_anon_bytes: anon,
        rss_file_bytes: file,
        rss_shmem_bytes: shmem,
    }
}

#[test]
fn maps_parser_keeps_file_mappings_and_skips_pseudo_or_deleted_entries() {
    let text = concat!(
        "00400000-00401000 r--p 00000000 08:01 123 /usr/bin/app\n",
        "00401000-00402000 rw-p 00000000 00:00 0 [heap]\n",
        "00402000-00403000 r--p 00000000 08:01 124 /usr/lib/libold.so (deleted)\n",
        "00403000-00404000 r--p 00000000 08:01 125 /usr/lib/lib with spaces.so\n",
    );
    let mappings = parse_proc_maps(text).expect("valid maps fixture");
    assert_eq!(
        mappings,
        vec![
            FileMapping {
                path: "/usr/bin/app".to_owned().into(),
                size_bytes: 4096,
            },
            FileMapping {
                path: "/usr/lib/lib with spaces.so".to_owned().into(),
                size_bytes: 4096,
            },
        ]
    );
}

#[test]
fn hybrid_pss_divides_file_rss_by_weighted_share_and_keeps_private_memory() {
    let mapping = FileMapping {
        path: "/usr/lib/libshared.so".to_owned().into(),
        size_bytes: 4096,
    };
    let mappings = vec![mapping.clone(), mapping];
    let shares = HashMap::from([(String::from("/usr/lib/libshared.so"), 2)]);

    // Two 4 KiB mappings shared by two processes produce a 1/2 file
    // charge. 100 bytes of anon + shmem remain private.
    assert_eq!(hybrid_pss(status(60, 600, 40), &mappings, &shares), Ok(400));
}

#[test]
fn hybrid_pss_does_not_need_maps_when_file_rss_is_zero() {
    let shares: HashMap<String, u32> = HashMap::new();
    assert_eq!(hybrid_pss(status(60, 0, 40), &[], &shares), Ok(100));
    assert_eq!(
        hybrid_pss(status(60, 600, 40), &[], &shares),
        Err(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn result_outcomes_keep_failures_visible() {
    assert_eq!(result_outcome(&Ok(0)), SourceOutcome::Available);
    assert_eq!(
        result_outcome(&Err(FailureKind::PermissionDenied)),
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
}
