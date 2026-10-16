# Original GreenLens synthetic geometry, Apache-2.0. Development-only authoring
# through the documented SketchUp Ruby API; never loaded by GreenLens runtime.
# Run once in a new disposable SketchUp document. Does not open existing models,
# erase document contents, fetch URLs or use an SDK DLL. Only new definitions
# below are saved; the template's unrelated objects are not included.
require 'json'
require 'fileutils'

root = File.join(ENV.fetch('LOCALAPPDATA'), 'GreenLens', 'verification', 'v0.8-independent-skp-fixture')
raise 'Existing fixture directory: preserve it and inspect the receipt' if File.exist?(root)
FileUtils.mkdir_p(root)
expected = {
  schema: 'greenlens.independent-skp-geometry.v1',
  source_axes: 'right-handed XYZ, Z up; coordinates below in metres',
  root_instances: 4, reachable_nested_definitions: 3, placed_instances_including_groups: 8,
  expanded_faces: 6, expanded_surface_area_m2: 64.0,
  bounds_m: { min: [0, 0, 0], max: [13, 13, 2] },
  definitions: {
    panel: { outer_m: [[0,0,0],[4,0,0],[4,3,0],[0,3,0]], hole_m: [[1,1,0],[2,1,0],[2,2,0],[1,2,0]], faces: 1, edges: 8, area_m2: 11.0 },
    concave: { outer_m: [[0,0,0],[3,0,0],[3,1,0],[1,1,0],[1,3,0],[0,3,0]], faces: 1, edges: 6, area_m2: 5.0 },
    nested: { children: [{ definition: 'panel', translation_m: [0,0,2] }, { definition: 'concave', translation_m: [5,0,0] }] }
  },
  occurrences: [
    { definition: 'nested', translation_m: [0,0,0], scale: [1,1,1], name: 'Repeated 同名', hidden: false },
    { definition: 'nested', translation_m: [12,0,0], scale: [-1,2,0.5], name: 'Repeated 同名', hidden: false },
    { definition: 'panel', translation_m: [0,10,0], scale: [1,1,1], name: 'Hidden occurrence', hidden: true },
    { definition: 'concave', translation_m: [10,10,0], scale: [1,1,1], name: 'Hidden tag occurrence', hidden_tag: true }
  ],
  tolerance: { coordinate_m: 1e-6, relative_coordinate: 1e-6, area_m2: 1e-6, relative_area: 1e-6 },
  notice: 'Expectations declared before native authoring and before any OpenSKP parsing. Hidden geometry remains part of expected complete geometry.'
}
File.write(File.join(root, 'expected-before-authoring.json'), JSON.pretty_generate(expected))
model = Sketchup.active_model
raise 'Use an untitled disposable document' unless model.path.empty?
defs = model.definitions
raise 'Synthetic definition names already exist' if defs['GreenLens fixture root']
panel = defs.add('GreenLens panel hole')
to_inches = ->(points) { points.map { |p| p.map { |v| v.m } } }
panel.entities.add_face(to_inches.call(expected[:definitions][:panel][:outer_m]))
inner = panel.entities.add_face(to_inches.call(expected[:definitions][:panel][:hole_m]))
# This removes only the newly created inner synthetic face, creating its hole.
inner.erase!
panel.entities.grep(Sketchup::Face).each { |f| f.reverse! if f.normal.z < 0 }
concave = defs.add('GreenLens concave')
face = concave.entities.add_face(to_inches.call(expected[:definitions][:concave][:outer_m]))
face.reverse! if face.normal.z < 0
nested = defs.add('GreenLens nested')
nested.entities.add_instance(panel, Geom::Transformation.translation([0,0,2.m]))
nested.entities.add_instance(concave, Geom::Transformation.translation([5.m,0,0]))
container = defs.add('GreenLens fixture root')
first = container.entities.add_instance(nested, Geom::Transformation.new)
first.name = 'Repeated 同名'
second = container.entities.add_instance(nested, Geom::Transformation.translation([12.m,0,0]) * Geom::Transformation.scaling(-1,2,0.5))
second.name = 'Repeated 同名'
hidden = container.entities.add_instance(panel, Geom::Transformation.translation([0,10.m,0]))
hidden.name = 'Hidden occurrence'
hidden.hidden = true
tagged = container.entities.add_instance(concave, Geom::Transformation.translation([10.m,10.m,0]))
tagged.name = 'Hidden tag occurrence'
tag = model.layers.add('GreenLens hidden synthetic tag')
tag.visible = false
tagged.layer = tag
audit = { application: Sketchup.version, authoring: 'original owned Ruby geometry via official application API',
  panel_faces: panel.entities.grep(Sketchup::Face).size, panel_edges: panel.entities.grep(Sketchup::Edge).size,
  panel_area_m2: panel.entities.grep(Sketchup::Face).sum(&:area) * 0.0254**2,
  concave_faces: concave.entities.grep(Sketchup::Face).size, concave_edges: concave.entities.grep(Sketchup::Edge).size,
  concave_area_m2: concave.entities.grep(Sketchup::Face).sum(&:area) * 0.0254**2,
  root_instances: container.entities.grep(Sketchup::ComponentInstance).size,
  occurrences: container.entities.grep(Sketchup::ComponentInstance).map { |i| { name: i.name, definition: i.definition.name, matrix_source_inches: i.transformation.to_a, hidden: i.hidden?, tag: i.layer.name, tag_visible: i.layer.visible? } } }
raise 'Authoring panel topology differs' unless audit[:panel_faces] == 1 && audit[:panel_edges] == 8 && (audit[:panel_area_m2] - 11).abs < 1e-9
raise 'Authoring concave topology differs' unless audit[:concave_faces] == 1 && audit[:concave_edges] == 6 && (audit[:concave_area_m2] - 5).abs < 1e-9
raise 'Authoring instance count differs' unless audit[:root_instances] == 4
[:legacy_2013, :modern].zip([Sketchup::Model::VERSION_2013, Sketchup::Model::VERSION_2021]).each do |label, version|
  path = File.join(root, "#{label}.skp")
  raise "Native save failed: #{label}" unless container.save_copy(path, version)
  audit[label] = { version_argument: version, bytes: File.size(path) }
end
File.write(File.join(root, 'native-authoring-receipt.json'), JSON.pretty_generate(audit))
puts JSON.pretty_generate(audit)
