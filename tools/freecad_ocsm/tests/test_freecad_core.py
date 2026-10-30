"""Real OCCT/FreeCAD tests. Skipped outside FreeCADCmd; no GUI/MCP required."""
from dataclasses import replace
import math
from types import SimpleNamespace
import unittest

try:
    import FreeCAD as App
    import Part
except ImportError:
    App = Part = None

from ocsm_export.api import build_scene, options_from_techdraw
from ocsm_export.curves import edge_curves
from ocsm_export.model import ExportError, ExportOptions, SectionSpec, STANDARD_VIEWS, ViewSpec
from ocsm_export.sources import collect_sources


def bounds(entities):
    points = [p for e in entities for p in e['preview_points']]
    return (min(p[0] for p in points), min(p[1] for p in points),
            max(p[0] for p in points), max(p[1] for p in points))


@unittest.skipUnless(App is not None, 'Run with tests/run_freecad.FCMacro for real FreeCAD tests')
class FreeCADCoreTests(unittest.TestCase):
    def setUp(self):
        self.doc = App.newDocument('OCSMExportCoreTest')

    def tearDown(self):
        App.closeDocument(self.doc.Name)

    def feature(self, name, shape):
        obj = self.doc.addObject('Part::Feature', name)
        obj.Shape = shape
        self.doc.recompute()
        return obj

    def block(self):
        return Part.makeBox(40, 30, 20).cut(
            Part.makeCylinder(4, 10, App.Vector(12, 0, 10), App.Vector(0, 1, 0)))

    def ring(self):
        return Part.makeCylinder(10, 20).cut(Part.makeCylinder(4, 20))

    def test_all_standard_views_and_visibility_modes(self):
        shape = self.block()
        for name, spec in STANDARD_VIEWS.items():
            for mode in ('visible', 'hidden'):
                with self.subTest(view=name, mode=mode):
                    scene = build_scene([shape], ExportOptions(views=(spec,), mode=mode))
                    self.assertTrue(scene['views'][0]['entities'])
                    self.assertFalse(scene['warnings'])
                    if mode == 'visible':
                        self.assertTrue(all(e['role'] == 'visible' for e in scene['views'][0]['entities']))

    def test_front_top_right_dimensions_are_model_scale(self):
        shape = Part.makeBox(40, 30, 20)
        for name, expected in [('front', (40, 20)), ('top', (40, 30)), ('right', (30, 20))]:
            with self.subTest(name=name):
                scene = build_scene([shape], ExportOptions(views=(STANDARD_VIEWS[name],)))
                x0,y0,x1,y1 = bounds(scene['views'][0]['entities'])
                self.assertAlmostEqual(x1-x0, expected[0], places=8)
                self.assertAlmostEqual(y1-y0, expected[1], places=8)

    def test_non_symmetric_world_coordinates_and_arbitrary_rotation(self):
        shape = Part.makeBox(40,30,20,App.Vector(100,200,300))
        spec = ViewSpec('rotated', (0,0,1), (0,1,0), 90)
        x0,y0,x1,y1 = bounds(build_scene([shape],ExportOptions(views=(spec,)))['views'][0]['entities'])
        for a,b in zip((x0,y0,x1,y1),(-230,100,-200,140)):
            self.assertAlmostEqual(a,b,places=7)

    def test_coincident_hidden_edges_do_not_duplicate_visible_edges(self):
        scene = build_scene([Part.makeBox(40,30,20)], ExportOptions(mode='hidden'))
        self.assertEqual(len(scene['views'][0]['entities']),4)
        self.assertFalse(any(e['role']=='hidden' for e in scene['views'][0]['entities']))

    def test_back_view_blind_hole_is_hidden_not_visible(self):
        options = ExportOptions(views=(STANDARD_VIEWS['back'],),mode='hidden')
        entities = build_scene([self.block()],options)['views'][0]['entities']
        circles = [e for e in entities if e['type']=='CIRCLE']
        self.assertEqual(len(circles),1)
        self.assertEqual(circles[0]['role'],'hidden')
        self.assertAlmostEqual(circles[0]['radius'],4)

    def test_multiple_parts_occlude_each_other_globally(self):
        front = Part.makeBox(40,10,30)
        behind = Part.makeBox(10,5,10,App.Vector(10,20,10))
        visible = build_scene([front,behind],ExportOptions())['views'][0]['entities']
        self.assertEqual(len(visible),4)  # not two separately projected rectangles
        hidden = build_scene([front,behind],ExportOptions(mode='hidden'))['views'][0]['entities']
        self.assertTrue(any(e['role']=='hidden' for e in hidden))

    def test_iso_preserves_ellipse_not_polyline(self):
        scene = build_scene([self.ring()],ExportOptions(views=(STANDARD_VIEWS['iso'],)))
        ellipses=[e for e in scene['views'][0]['entities'] if e['type']=='ELLIPSE']
        self.assertTrue(ellipses)
        for e in ellipses:
            self.assertAlmostEqual(e['ratio'],1/math.sqrt(3),places=7)
        self.assertFalse(scene['warnings'])

    def test_annular_section_has_true_outer_and_inner_loops(self):
        options=ExportOptions(views=(STANDARD_VIEWS['top'],),mode='section',section=SectionSpec((0,0,10),(0,0,1)))
        scene=build_scene([self.ring()],options)
        hatches=scene['views'][0]['hatches']
        self.assertEqual(len(hatches),1)
        loops=hatches[0]['loops']
        self.assertEqual(sorted(l['is_outer'] for l in loops),[False,True])
        self.assertEqual(sorted(l['edges'][0]['radius'] for l in loops),[4,10])
        self.assertEqual({e['role'] for e in scene['views'][0]['entities']},{'visible'})

    def test_iso_section_hatch_boundary_is_analytic_ellipse(self):
        options=ExportOptions(views=(STANDARD_VIEWS['iso'],),mode='section',section=SectionSpec((0,0,10),(0,0,1)))
        scene=build_scene([self.ring()],options)
        loops=scene['views'][0]['hatches'][0]['loops']
        for loop in loops:
            edge=loop['edges'][0]
            self.assertEqual(edge['type'],'ELLIPSE')
            self.assertAlmostEqual(edge['ratio'],1/math.sqrt(3),places=8)
            ax,ay=edge['major_axis']; a=math.hypot(ax,ay); b=a*edge['ratio']
            for px,py in edge['preview_points']:
                x,y=px-edge['center'][0],py-edge['center'][1]
                u=(x*ax+y*ay)/a;v=(-x*ay+y*ax)/a
                self.assertAlmostEqual((u/a)**2+(v/b)**2,1,places=7)

    def test_disconnected_faces_and_part_identity_are_preserved(self):
        shape = Part.makeCompound([Part.makeBox(10,10,20),Part.makeBox(10,10,20,App.Vector(30,0,0))])
        other = Part.makeBox(10,10,20,App.Vector(60,0,0))
        options=ExportOptions(views=(STANDARD_VIEWS['top'],),mode='section',section=SectionSpec((0,0,10),(0,0,1)))
        scene=build_scene([shape,other],options)
        hatches=scene['views'][0]['hatches']
        self.assertEqual(len(hatches),3)
        self.assertEqual([h['source_index'] for h in hatches],[0,0,1])
        self.assertEqual([h['angle_deg'] for h in hatches],[45,45,135])

    def test_nonintersecting_edgeon_and_wrong_side_sections_fail(self):
        cases=[(STANDARD_VIEWS['top'],SectionSpec((0,0,50),(0,0,1))),
               (STANDARD_VIEWS['front'],SectionSpec((0,0,10),(0,0,1))),
               (STANDARD_VIEWS['bottom'],SectionSpec((0,0,10),(0,0,1)))]
        for spec,cut in cases:
            with self.subTest(view=spec.name,cut=cut):
                with self.assertRaises(ExportError):
                    build_scene([self.ring()],ExportOptions(views=(spec,),mode='section',section=cut))
        # Explicit opposite keep side + opposite view is valid.
        scene=build_scene([self.ring()],ExportOptions(views=(STANDARD_VIEWS['bottom'],),mode='section',
                         section=SectionSpec((0,0,10),(0,0,1),keep='positive')))
        self.assertTrue(scene['views'][0]['hatches'])

    def test_section_requires_closed_solids(self):
        with self.assertRaises(ExportError):
            build_scene([Part.makePlane(10,10)],ExportOptions(mode='section',section=SectionSpec()))
        mixed=Part.makeCompound([Part.makeBox(10,10,20),Part.makeLine(App.Vector(30,0,0),App.Vector(40,0,0))])
        with self.assertRaises(ExportError):
            build_scene([mixed],ExportOptions(views=(STANDARD_VIEWS['top'],),mode='section',
                        section=SectionSpec((0,0,10),(0,0,1))))

    def test_compound_with_retained_and_cut_solids_hatches_only_cut_solid(self):
        retained=Part.makeBox(10,10,10,App.Vector(0,0,0))   # wholly on kept side
        crossing=Part.makeBox(10,10,10,App.Vector(20,0,5))  # crosses z=10
        shape=Part.makeCompound([retained,crossing])
        scene=build_scene([shape],ExportOptions(views=(STANDARD_VIEWS['top'],),mode='section',
                         section=SectionSpec((0,0,10),(0,0,1))))
        hatches=scene['views'][0]['hatches']
        self.assertEqual(len(hatches),1)
        points=[p for loop in hatches[0]['loops'] for e in loop['edges']
                for p in ([e['start'],e['end']] if e['type']=='LINE' else e.get('preview_points',[]))]
        self.assertTrue(points)
        self.assertGreaterEqual(min(p[0] for p in points),20-1e-6)

    def test_identical_shapes_are_exported_once(self):
        first=self.feature('SameA',Part.makeBox(10,10,10))
        second=self.feature('SameB',Part.makeBox(10,10,10))
        sources,_=collect_sources([first,second])
        self.assertEqual(len(sources),1)
        third=self.feature('Moved',Part.makeBox(10,10,10,App.Vector(50,0,0)))
        sources,_=collect_sources([first,third])
        self.assertEqual(len(sources),2)

    def test_section_hatch_can_be_disabled_without_losing_outline(self):
        options=ExportOptions(views=(STANDARD_VIEWS['top'],),mode='section',
                              section=SectionSpec((0,0,10),(0,0,1),hatch_enabled=False))
        scene=build_scene([self.ring()],options)
        self.assertEqual(scene['views'][0]['hatches'],[])
        self.assertTrue(scene['views'][0]['entities'])

    def test_degree_one_hlr_edges_become_merged_lines(self):
        shape=Part.makeCylinder(25,60).fuse(Part.makeCylinder(20,50,App.Vector(0,0,60)))
        scene=build_scene([shape],ExportOptions(views=(STANDARD_VIEWS['front'],)))
        entities=scene['views'][0]['entities']
        self.assertEqual({e['type'] for e in entities},{'LINE'})
        self.assertEqual(len(entities),7)
        step=[e for e in entities
              if abs(e['start'][1]-60)<1e-6 and abs(e['end'][1]-60)<1e-6]
        self.assertEqual(len(step),1)
        self.assertAlmostEqual(min(step[0]['start'][0],step[0]['end'][0]),-25)
        self.assertAlmostEqual(max(step[0]['start'][0],step[0]['end'][0]),25)

    def test_per_view_hidden_and_section_modes_are_independent(self):
        options=ExportOptions(views=(replace(STANDARD_VIEWS['front'],mode='visible'),
                                     replace(STANDARD_VIEWS['top'],mode='section')),
                              mode='visible',section=SectionSpec((0,0,10),(0,0,1)))
        scene=build_scene([self.ring()],options)
        self.assertEqual(len(scene['views']),2)
        self.assertEqual(scene['views'][0]['hatches'],[])
        self.assertEqual(len(scene['views'][1]['hatches']),1)
        self.assertTrue(scene['views'][0]['entities'])

    def test_retained_but_uncut_part_is_not_hatched(self):
        ring=self.ring()
        uncut=Part.makeBox(5,5,5,App.Vector(30,0,0))
        scene=build_scene([ring,uncut],ExportOptions(views=(STANDARD_VIEWS['top'],),mode='section',
                         section=SectionSpec((0,0,10),(0,0,1))))
        self.assertEqual(len(scene['views'][0]['hatches']),1)
        self.assertEqual(scene['views'][0]['hatches'][0]['source_index'],0)

    def test_oblique_full_section_preserves_elliptical_rings(self):
        ring=Part.makeCylinder(10,40).cut(Part.makeCylinder(4,40))
        view=ViewSpec('oblique',(1,0,1),(0,0,1))
        options=ExportOptions(views=(view,),mode='section',section=SectionSpec((0,0,20),(1,0,1)))
        scene=build_scene([ring],options)
        loops=scene['views'][0]['hatches'][0]['loops']
        self.assertEqual(len(loops),2)
        for loop in loops:
            self.assertEqual(loop['edges'][0]['type'],'ELLIPSE')
            self.assertAlmostEqual(loop['edges'][0]['ratio'],1/math.sqrt(2),places=7)

    def test_global_parent_placement_is_applied_once(self):
        parent=self.doc.addObject('App::Part','Assembly')
        parent.Placement=App.Placement(App.Vector(100,200,300),App.Rotation(App.Vector(0,0,1),90))
        child=self.feature('Child',Part.makeBox(10,20,30))
        child.Placement.Base=App.Vector(1,2,3);parent.addObject(child);self.doc.recompute()
        sources,_=collect_sources([child])
        center=sources[0].shape.BoundBox.Center
        for actual,expected in zip(tuple(center),(88,206,318)):
            self.assertAlmostEqual(actual,expected,places=7)
        container_sources,_=collect_sources([parent,child])
        self.assertEqual(len(container_sources),1)
        self.assertAlmostEqual(container_sources[0].shape.Volume,6000)
        from ocsm_export.gui import selection_center
        for actual,expected in zip(selection_center([parent]),(88,206,318)):
            self.assertAlmostEqual(actual,expected,places=7)

    def test_link_instance_and_ancestor_placement(self):
        source=self.feature('Source',Part.makeBox(10,20,30))
        link=self.doc.addObject('App::Link','Instance');link.setLink(source)
        link.LinkPlacement.Base=App.Vector(500,600,700)
        parent=self.doc.addObject('App::Part','Assembly');parent.Placement.Base=App.Vector(100,200,300)
        parent.addObject(link);self.doc.recompute()
        sources,_=collect_sources([link])
        for actual,expected in zip(tuple(sources[0].shape.BoundBox.Center),(605,810,1015)):
            self.assertAlmostEqual(actual,expected,places=7)

    def test_body_exports_only_tip_not_feature_history(self):
        body=self.doc.addObject('PartDesign::Body','Body')
        history=body.newObject('PartDesign::Feature','History');history.Shape=Part.makeBox(100,100,100)
        tip=body.newObject('PartDesign::Feature','Tip');tip.Shape=Part.makeBox(10,20,30)
        body.Tip=tip;self.doc.recompute()
        sources,_=collect_sources([body])
        self.assertEqual(len(sources),1)
        self.assertAlmostEqual(sources[0].shape.Volume,6000)

    def test_no_document_mutation_and_only_selected_scope(self):
        selected=self.feature('Selected',Part.makeBox(10,20,30))
        self.feature('Unselected',Part.makeBox(500,500,500))
        before=[(o.Name,str(getattr(o,'Placement',None)),str(o.State)) for o in self.doc.Objects]
        scene=build_scene([selected],ExportOptions())
        after=[(o.Name,str(getattr(o,'Placement',None)),str(o.State)) for o in self.doc.Objects]
        self.assertEqual(before,after)
        self.assertEqual([s['name'] for s in scene['metadata']['sources']],['Selected'])
        self.assertEqual(bounds(scene['views'][0]['entities']),(0,0,10,30))

    def test_empty_invalid_unrecomputed_and_missing_source_fail(self):
        for selection in ([],[Part.Shape()],[self.doc.addObject('App::FeaturePython','NoShape')]):
            with self.assertRaises(ExportError):
                collect_sources(selection)
        obj=self.doc.addObject('Part::Box','Unrecomputed')
        with self.assertRaises(ExportError):
            collect_sources([obj])

    def test_curve_arc_direction_and_trimmed_spline(self):
        arc=Part.Arc(App.Vector(1,0,0),App.Vector(0,1,0),App.Vector(-1,0,0)).toShape()
        a=edge_curves(arc,'visible',ExportOptions(),[])[0]
        self.assertEqual(a['type'],'ARC');self.assertTrue(a['ccw'])
        arc.reverse();b=edge_curves(arc,'visible',ExportOptions(),[])[0]
        self.assertFalse(b['ccw'])
        self.assertAlmostEqual(b['start_angle'],180)
        spline=Part.BSplineCurve();spline.interpolate([App.Vector(0,0,0),App.Vector(3,5,0),App.Vector(7,-2,0),App.Vector(10,0,0)])
        edge=spline.toShape(spline.FirstParameter+1,spline.LastParameter-1)
        data=edge_curves(edge,'visible',ExportOptions(),[])[0]
        self.assertEqual(data['type'],'SPLINE')
        self.assertEqual(len(data['knots']),len(data['control_points'])+data['degree']+1)
        self.assertAlmostEqual(data['knots'][0],edge.FirstParameter)
        self.assertAlmostEqual(data['knots'][-1],edge.LastParameter)

    def test_unsupported_curve_requires_explicit_approximation(self):
        edge=SimpleNamespace(Curve=SimpleNamespace(),Length=10,Orientation='Forward',
                             discretize=lambda **kw:[App.Vector(0,0,0),App.Vector(10,0,0)])
        with self.assertRaises(ExportError):
            edge_curves(edge,'visible',ExportOptions(),[])
        warnings=[]
        result=edge_curves(edge,'visible',ExportOptions(allow_approximation=True),warnings)
        self.assertEqual(result[0]['type'],'LINE');self.assertTrue(warnings)

    def test_inherited_view_options_remove_paper_scale_and_keep_independent_hlr_flags(self):
        obj=self.feature('Source',self.block())
        view=SimpleNamespace(TypeId='TechDraw::DrawViewPart',Name='View',Source=[obj],
                             XSource=[],Direction=App.Vector(0,1,0),XDirection=App.Vector(1,0,0),
                             Rotation=30,Scale=0.5,Perspective=False,SmoothVisible=True,HardHidden=True,
                             SmoothHidden=False,SeamVisible=False,SeamHidden=False,
                             CosmeticEdges=[],CenterLines=[],GeomFormats=[])
        options=options_from_techdraw(view)
        self.assertTrue(options.include_smooth);self.assertFalse(options.hidden_smooth)
        scene=build_scene([view],options)
        lines=[e for e in scene['views'][0]['entities'] if e['role']=='visible' and e['type']=='LINE']
        self.assertEqual(sorted(round(math.dist(e['start'],e['end']),7) for e in lines),[20,20,40,40])
        self.assertEqual(scene['metadata']['ignored_paper_scale'],0.5)
        view.CosmeticEdges=[object()]
        with self.assertRaises(ExportError):
            build_scene([view],options)
        scene=build_scene([view],replace(options,include_cosmetics=False))
        self.assertTrue(scene['warnings'])
        view.TypeId='TechDraw::DrawViewDetail'
        with self.assertRaises(ExportError):
            options_from_techdraw(view)


if __name__=='__main__':
    unittest.main()
