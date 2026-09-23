use super::*;

pub(in crate::nif2map) fn reconstruct(
    meshes: &[VisualMesh],
    options: &Options,
) -> Result<Reconstruction, Error> {
    let mut partitions: BTreeMap<(ScopeId, NifState), Vec<&VisualMesh>> = BTreeMap::new();
    for mesh in meshes {
        partitions
            .entry((mesh.scope, mesh.nif_state.clone()))
            .or_default()
            .push(mesh);
    }
    let partitions: Vec<_> = partitions.into_iter().collect();
    let partials: Vec<_> = partitions
        .into_iter()
        .map(|((scope, nif_state), partition)| {
            let mut partial = reconstruct_partition(&partition, options)?;
            for brush in &mut partial.brushes {
                brush.scope = scope;
                brush.nif_state = nif_state.clone();
            }
            Ok::<_, Error>((partial, scope, nif_state))
        })
        .collect();
    let mut result = Reconstruction::default();
    for partial in partials {
        let (partial, _scope, _nif_state) = partial?;
        result.brushes.extend(partial.brushes);
        result.used_shapes.extend(partial.used_shapes);
        result.recognizers.extend(partial.recognizers);
        result.warnings.extend(partial.warnings);
        result.uv_max_error = result.uv_max_error.max(partial.uv_max_error);
        result.timings.add_assign(partial.timings);
    }
    if result.brushes.len() > options.max_brushes {
        return Err(Error::Reconstruction(format!(
            "reconstruction produced {} brushes, exceeding --max-brushes {}",
            result.brushes.len(),
            options.max_brushes
        )));
    }
    Ok(result)
}

fn reconstruct_partition(
    meshes: &[&VisualMesh],
    options: &Options,
) -> Result<Reconstruction, Error> {
    let mut result = Reconstruction::default();
    let mut remaining: HashSet<usize> = meshes.iter().map(|mesh| mesh.block).collect();
    let sweep_started = Instant::now();
    let sweeps: HashMap<usize, Option<Sweep>> = meshes
        .iter()
        .map(|mesh| (mesh.block, detect_sweep(mesh)))
        .collect();
    result.timings.sweep_analysis = sweep_started.elapsed();
    let mut seeds = meshes.to_vec();
    seeds.sort_by(|lhs, rhs| {
        rhs.triangles
            .len()
            .cmp(&lhs.triangles.len())
            .then(lhs.block.cmp(&rhs.block))
    });
    let structural_started = Instant::now();
    for seed in seeds {
        process_seed(seed, meshes, &sweeps, options, &mut remaining, &mut result);
    }
    result.timings.structural_recognition = structural_started.elapsed();
    let fallback_started = Instant::now();
    apply_fallback(options, meshes, &mut remaining, &mut result)?;
    result.timings.planar_fallback = fallback_started.elapsed();
    report_unsupported(meshes, &remaining, &mut result);
    if result.brushes.len() > options.max_brushes {
        return Err(Error::Reconstruction(format!(
            "reconstruction produced {} brushes, exceeding --max-brushes {}",
            result.brushes.len(),
            options.max_brushes
        )));
    }
    if options.validate {
        let validation_started = Instant::now();
        validate_reconstruction(&mut result)?;
        result.timings.brush_validation = validation_started.elapsed();
    }
    let uv_started = Instant::now();
    result.uv_max_error = max_uv_error(meshes);
    result.timings.uv_diagnostics = uv_started.elapsed();
    Ok(result)
}

fn process_seed(
    seed: &VisualMesh,
    meshes: &[&VisualMesh],
    sweeps: &HashMap<usize, Option<Sweep>>,
    options: &Options,
    remaining: &mut HashSet<usize>,
    result: &mut Reconstruction,
) {
    if !remaining.contains(&seed.block) {
        return;
    }
    let Some(sweep) = sweeps.get(&seed.block).and_then(|sweep| sweep.as_ref()) else {
        return;
    };
    let seed_group = [seed];
    if let Some((brushes, report)) = exact_extrusion(&seed_group, sweep, &options.skip_material) {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        result.used_shapes.insert(seed.block);
        remaining.remove(&seed.block);
        return;
    }
    let group = group_open_shell_shapes(seed, sweep, remaining, meshes, sweeps);
    if let Some((brushes, report)) = swept_shell(
        &group,
        sweep,
        options.shell_thickness,
        &options.skip_material,
    ) {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        for mesh in group {
            result.used_shapes.insert(mesh.block);
            remaining.remove(&mesh.block);
        }
    }
}

fn apply_fallback(
    options: &Options,
    meshes: &[&VisualMesh],
    remaining: &mut HashSet<usize>,
    result: &mut Reconstruction,
) -> Result<(), Error> {
    if options.fallback == "skip" {
        return Ok(());
    }
    if options.fallback != "planar-prisms" {
        return Err(Error::Reconstruction(format!(
            "unknown fallback mode {:?}",
            options.fallback
        )));
    }
    let mut fallback_meshes: Vec<_> = meshes
        .iter()
        .copied()
        .filter(|mesh| remaining.contains(&mesh.block))
        .collect();
    fallback_meshes.sort_by_key(|mesh| mesh.block);
    let mut fallback_results: Vec<_> = fallback_meshes
        .iter()
        .map(|mesh| {
            (
                mesh.block,
                planar_fallback(mesh, options.fallback_thickness, &options.skip_material),
            )
        })
        .collect();
    fallback_results.sort_by_key(|(block, _)| *block);
    for (block, fallback) in fallback_results {
        let mesh = fallback_meshes
            .binary_search_by_key(&block, |mesh| mesh.block)
            .ok()
            .and_then(|index| fallback_meshes.get(index))
            .expect("fallback result should have a source mesh");
        match fallback {
            Ok((brushes, report)) if !brushes.is_empty() => {
                result.brushes.extend(brushes);
                result.recognizers.push(report);
                result.used_shapes.insert(block);
                remaining.remove(&block);
            }
            Ok(_) => {}
            Err(error) => result.warnings.push(format!(
                "shape {} {:?}: fallback failed: {error}",
                block, mesh.name
            )),
        }
    }
    Ok(())
}

fn report_unsupported(
    meshes: &[&VisualMesh],
    remaining: &HashSet<usize>,
    result: &mut Reconstruction,
) {
    let mut unsupported: Vec<_> = meshes
        .iter()
        .copied()
        .filter(|mesh| remaining.contains(&mesh.block))
        .collect();
    unsupported.sort_by_key(|mesh| mesh.block);
    for mesh in unsupported {
        result.warnings.push(format!(
            "shape {} {:?}: unsupported/unreconstructed ({} verts, {} triangles)",
            mesh.block,
            mesh.name,
            mesh.vertices.len(),
            mesh.triangles.len()
        ));
    }
}

fn max_uv_error(meshes: &[&VisualMesh]) -> f64 {
    meshes
        .iter()
        .filter(|mesh| mesh.uvs.is_some())
        .flat_map(|mesh| (0..mesh.triangles.len()).map(move |index| (mesh, index)))
        .filter_map(|(mesh, index)| triangle_projection(mesh, index))
        .map(|projection| projection.max_error)
        .fold(0.0, f64::max)
}
