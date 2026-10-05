#!/usr/bin/env python3
"""Fresh real voice tests with exact cached dependency artifacts, no Cargo/sys-dev rebuild."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

REPO = Path('/workspace/Serein')
ROOT = Path(__file__).resolve().parent
NATIVE_ROOT = Path('/workspace/scratch/serein-openh264-isolation')
UI_CACHE = Path('/workspace/scratch/serein-ui-preview/target/debug')
CACHE = REPO / 'target/debug'
FINGERPRINT = CACHE / '.fingerprint/discord-voice-b7b7e1441434b65b/test-lib-discord_voice.json'
RUSTC = Path('/home/agent/.cargo/bin/rustc')


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dependencies(fingerprint=FINGERPRINT, cache=CACHE, names=None):
    """Cargo serializes dependency fingerprint values as little-endian u64 hex."""
    fp = json.loads(fingerprint.read_text())
    result = {}
    for _, name, _, key in fp['deps']:
        if name == 'build_script_build' or names is not None and name not in names:
            continue
        candidates = []
        for candidate in (cache / '.fingerprint').glob(f'*/lib-{name}'):
            if int.from_bytes(bytes.fromhex(candidate.read_text().strip()), 'little') != key:
                continue
            suffix = candidate.parent.name.rsplit('-', 1)[-1]
            artifact = cache / 'deps' / f'lib{name}-{suffix}.rlib'
            if artifact.is_file():
                candidates.append(artifact)
        if len(candidates) != 1:
            raise ValueError(f'Expected one exact cached dependency for {name}: {candidates}')
        artifact = candidates[0]
        result[name] = {'artifact': str(artifact), 'sha256': sha256(artifact), 'fingerprint': key}
    return result


def native_aliases(directory):
    """Provide development link names pointing only to existing runtime libraries."""
    directory.mkdir(exist_ok=True)
    output = subprocess.check_output(['/sbin/ldconfig', '-p'], text=True)
    for line in output.splitlines():
        if ' => ' not in line or 'x86-64' not in line:
            continue
        soname = line.strip().split()[0]
        if '.so.' not in soname:
            continue
        source = Path(line.split(' => ', 1)[1])
        alias = directory / (soname.split('.so.', 1)[0] + '.so')
        if source.is_file() and not alias.exists():
            alias.symlink_to(source)
    for source in sorted((NATIVE_ROOT / 'native-runtime-packages/usr/lib/x86_64-linux-gnu').glob('lib*.so.*')):
        if not source.is_file():
            continue
        alias = directory / (source.name.split('.so.', 1)[0] + '.so')
        if not alias.exists():
            alias.symlink_to(source)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path)
    parser.add_argument('--ui-fingerprint', type=Path, required=True)
    parser.add_argument('--metadata-only', action='store_true')
    args = parser.parse_args()
    if not args.metadata_only and args.prefix is None:
        parser.error('--prefix is required for actual native test builds')
    out = ROOT / 'voice-current-source-build'
    out.mkdir(exist_ok=True)
    deps = dependencies()
    deps.update(dependencies(args.ui_fingerprint, UI_CACHE, {'model', 'client_core'}))
    if not {'model', 'client_core'}.issubset(deps):
        raise ValueError('Current UI model/client_core dependencies are missing')
    (out / 'dependencies.json').write_text(json.dumps(deps, indent=2) + '\n')
    version = subprocess.check_output([str(RUSTC), '--version'], cwd=REPO, text=True).strip()
    if not version.startswith('rustc 1.98.1 '):
        raise ValueError(f'Expected pinned Rust 1.98.1, got {version}')
    commands = []
    native_flags = []
    if not args.metadata_only:
        prefix = args.prefix.resolve()
        if not (prefix / 'include/libavcodec/avcodec.h').is_file():
            raise ValueError(f'Missing completed native FFmpeg prefix: {prefix}')
        build_fp = CACHE / '.fingerprint/discord-voice-6eb1502c7588a09e/build-script-build-script-build.json'
        build_deps = dependencies(build_fp)
        (out / 'build-script-dependencies.json').write_text(json.dumps(build_deps, indent=2) + '\n')
        build_cmd = [str(RUSTC), '--crate-name', 'build_script_build', '--edition=2024',
                     str(REPO / 'crates/discord-voice/build.rs'),
                     '-L', 'dependency=' + str(CACHE / 'deps'), '-o', str(out / 'voice-build-script')]
        for name, data in build_deps.items():
            build_cmd += ['--extern', name + '=' + data['artifact']]
        print('Compiling actual current build.rs', flush=True)
        subprocess.run(build_cmd, cwd=REPO, check=True)
        commands.append(build_cmd)
        cfg_lines = subprocess.check_output([str(RUSTC), '--print', 'cfg'], cwd=REPO, text=True).splitlines()
        build_env = dict(os.environ, OUT_DIR=str(out), OPT_LEVEL='3', DEBUG='false',
                         PROFILE='debug', TARGET='x86_64-unknown-linux-gnu', HOST='x86_64-unknown-linux-gnu',
                         NUM_JOBS='3', CARGO_MANIFEST_DIR=str(REPO / 'crates/discord-voice'),
                         CARGO_ENCODED_RUSTFLAGS='', FFMPEG_DIR=str(prefix))
        for line in cfg_lines:
            name, _, value = line.partition('=')
            key = 'CARGO_CFG_' + name.upper()
            value = value.strip('"') if value else '1'
            if key in build_env:
                build_env[key] += ',' + value
            else:
                build_env[key] = value
        print('Running actual build.rs / compiling fresh C shim', flush=True)
        build_result = subprocess.run([str(out / 'voice-build-script')], cwd=REPO / 'crates/discord-voice',
                                      env=build_env, check=True, stdout=subprocess.PIPE, text=True)
        (out / 'build-script-output.txt').write_text(build_result.stdout)
        for line in build_result.stdout.splitlines():
            if line.startswith('cargo:rustc-link-arg='):
                native_flags += ['-C', 'link-arg=' + line.split('=', 1)[1]]
            elif line.startswith('cargo:rustc-link-search='):
                native_flags += ['-L', line.split('=', 1)[1]]
            elif line.startswith('cargo:rustc-link-lib='):
                native_flags += ['-l', line.split('=', 1)[1]]
    cmd = [str(RUSTC), '--crate-name', 'discord_voice', '--edition=2024', '--test',
           str(REPO / 'crates/discord-voice/src/lib.rs'), '--deny=unsafe_code',
           '-C', 'metadata=serein_hardware_capabilities',
           '-C', 'opt-level=3', '-C', 'debug-assertions=on', '-C', 'overflow-checks=on',
           '-C', 'debuginfo=0', '-C', 'codegen-units=16',
           '-L', 'dependency=' + str(CACHE / 'deps'),
           '-L', 'dependency=' + str(UI_CACHE / 'deps')]
    for name, data in deps.items():
        cmd += ['--extern', name + '=' + data['artifact']]
    if args.metadata_only:
        cmd += ['--emit=metadata', '-o', str(out / 'discord_voice.rmeta')]
    else:
        aliases = out / 'native-runtime-aliases'
        native_aliases(aliases)
        cmd += ['-L', 'native=' + str(aliases), *native_flags,
                '-o', str(out / 'discord-voice-current-tests')]
        for archive in (CACHE / 'build').glob('*/out/**/*.a'):
            if archive.name != 'libserein_avc.a':
                cmd += ['-L', 'native=' + str(archive.parent)]
    commands.append(cmd)
    library_cmd = None
    if not args.metadata_only:
        library_cmd = list(cmd)
        library_cmd.remove('--test')
        library_cmd += ['--crate-type=rlib']
        library_cmd[library_cmd.index('-o') + 1] = str(out / 'libdiscord_voice_hw.rlib')
        commands.append(library_cmd)
        (out / 'link-context.json').write_text(json.dumps({
            'voice_library': str(out / 'libdiscord_voice_hw.rlib'),
            'dependency_search': [str(CACHE / 'deps'), str(UI_CACHE / 'deps')],
            'native_flags': native_flags,
            'native_search': [str(aliases), *sorted({str(archive.parent) for archive in (CACHE / 'build').glob('*/out/**/*.a') if archive.name != 'libserein_avc.a'})],
            'ui_fingerprint': str(args.ui_fingerprint),
        }, indent=2) + '\n')
    (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
    sources = sorted((REPO / 'crates/discord-voice/src').rglob('*'))
    sources += [REPO / 'vendor/hpke-rs/src/serein_sha3.rs',
                REPO / 'crates/discord-voice/build.rs', REPO / 'crates/model/src/voice_settings.rs',
                REPO / 'Cargo.lock', Path(__file__)]
    metadata = {'rustc': version, 'metadata_only': args.metadata_only,
                'sources_sha256': {str(p.relative_to(REPO)) if p.is_relative_to(REPO) else str(p):
                                   sha256(p) for p in sources if p.is_file()}}
    if args.prefix:
        metadata['ffmpeg_prefix'] = str(args.prefix.resolve())
    (out / 'build-metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    env = dict(os.environ, CARGO_MANIFEST_DIR=str(REPO / 'crates/discord-voice'),
               CARGO_PKG_VERSION='0.1.0', CARGO_PKG_NAME='discord-voice')
    for command in commands[-(2 if library_cmd else 1):]:
        print('Compiling and linking actual current voice crate:', command[0], flush=True)
        subprocess.run(command, cwd=REPO, env=env, check=True)
    metadata['built_artifacts_sha256'] = {
        p.name: sha256(p) for p in out.iterdir()
        if p.is_file() and p.name in {'voice-build-script', 'libserein_avc.a',
                                      'discord-voice-current-tests', 'discord_voice.rmeta', 'libdiscord_voice_hw.rlib'}}
    (out / 'build-metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print('Current source build complete:', out, flush=True)


if __name__ == '__main__':
    main()
