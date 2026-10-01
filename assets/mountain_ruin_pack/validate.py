"""Independent delivery-contract checks. Run: python validate.py. Exit 1 on failure."""
from pathlib import Path
import json, sys
from PIL import Image

ROOT=Path(__file__).resolve().parent
EXPECTED={
 'player.png':(384,256),'terrain.png':(128,64),'palette.png':(32,1),
 'objects/spikes.png':(16,16),'objects/saw.png':(128,32),'objects/spring.png':(48,16),
 'objects/dash_crystal.png':(128,16),'objects/collectible.png':(96,16),'objects/checkpoint.png':(112,32),
 'objects/goal.png':(192,32),'objects/moving_platform.png':(48,8),'objects/crumble_platform.png':(160,8),
 'objects/pressure_plate.png':(64,8),'objects/gate_block.png':(16,16),'objects/timed_spikes.png':(192,32),
 'objects/particles.png':(64,8),'enemies/walker.png':(192,16),'enemies/flyer.png':(192,16),
 'enemies/shooter.png':(128,16),'enemies/shot.png':(16,8),
 'background/sky.png':(8,224),'background/far.png':(400,224),'background/mid.png':(400,224),
 'background/near.png':(400,224),'ui/panel.png':(48,48),'ui/button.png':(144,16),
 'ui/logo.png':(96,80),'ui/icons.png':(80,16)}
PLAYER=[('idle',8,8,True),('run',12,14,True),('jump_rise',2,10,True),('fall',2,10,True),
        ('dash',4,20,True),('wall_slide',2,6,True),('hurt',4,16,False),('appear',6,16,False)]
FAIL=[];CHECKS=0
def check(ok,msg):
    global CHECKS
    CHECKS+=1
    if not ok:FAIL.append(msg)
def cell(a,i,cw,ch):
    cols=a.width//cw;x=i%cols*cw;y=i//cols*ch
    return a.crop((x,y,x+cw,y+ch))
def opaque(a):return [(x,y,c) for y in range(a.height) for x in range(a.width) if (c:=a.getpixel((x,y)))[3]]
def main():
    master=[tuple(bytes.fromhex(s.strip().lstrip('#'))) for s in (ROOT/'palette.hex').read_text().splitlines() if s.strip()]
    check(len(master)<=32 and len(master)==len(set(master)),'Palette must contain <=32 unique colours')
    palette=set(master);acc=set(master[28:31]);allcol=set();images={}
    manifest=json.loads((ROOT/'manifest.json').read_text());sheets={s['file']:s for s in manifest['sheets']}
    check(set(sheets)==set(EXPECTED)-{'palette.png'},'Manifest asset set differs from specification')
    found={str(p.relative_to(ROOT)).replace('\\','/') for p in ROOT.rglob('*.png') if 'previews' not in p.parts}
    check(found==set(EXPECTED),f'Unexpected or missing native PNG files: {found^set(EXPECTED)}')
    for name,size in EXPECTED.items():
        path=ROOT/name
        if not path.exists():check(False,f'Missing {name}');continue
        a=Image.open(path);images[name]=a
        check(a.size==size,f'{name}: expected {size}, got {a.size}')
        check(a.mode=='RGBA',f'{name}: must be RGBA')
        pixels=list(a.getdata());check(all(p[3] in (0,255) for p in pixels),f'{name}: fractional alpha')
        colours={p[:3] for p in pixels if p[3]};allcol|=colours
        check(colours<=palette,f'{name}: non-palette colours {colours-palette}')
        if name not in ['player.png','palette.png']:check(not colours&acc,f'{name}: exclusive player accent used')
        if name in sheets:
            sh=sheets[name];cw=sh['cell_width'];ch=sh['cell_height']
            check(a.width%cw==0 and a.height%ch==0,f'{name}: partial cells')
            check((sh['width'],sh['height'])==size,f'{name}: manifest dimensions mismatch')
            for an in sh['animations']:
                check(all(k in an for k in ['name','first_frame','frame_count','fps','loop','pivot']),f'{name}: incomplete animation metadata')
                check(an['first_frame']+an['frame_count']<=a.width//cw*(a.height//ch),f'{name}: animation outside sheet')
                check(an['fps']>0,f'{name}: invalid fps')
    check(len(allcol)<=32,f'Pack uses {len(allcol)} colours')
    check(list(images['palette.png'].getdata())==[c+(255,) for c in master],'Palette swatch ordering differs from hex')
    a=images['player.png'];grounded={'idle','run','dash','wall_slide','hurt'}
    check(acc<={p[:3] for p in a.getdata() if p[3]},'Player does not use all three dedicated accent colours')
    for row,(name,n,fps,loop) in enumerate(PLAYER):
        meta=sheets['player.png']['animations'][row]
        check((meta['name'],meta['row'],meta['first_frame'],meta['frame_count'],meta['fps'],meta['loop'])==(name,row,row*12,n,fps,loop),f'Player {name}: wrong frame order/timing')
        counts=[]
        for f in range(12):
            frame=cell(a,row*12+f,32,32);px=opaque(frame)
            if f>=n:check(not px,f'Player {name}: unused cell {f} is not empty');continue
            check(bool(px),f'Player {name} {f}: empty frame');counts.append(len(px))
            check(all(x not in (0,31) and y!=0 for x,y,c in px),f'Player {name} {f}: forbidden cell border contact')
            if name in grounded:
                check(max(y for x,y,c in px)==31,f'Player {name} {f}: feet not on y=31')
                body=[x for x,y,c in px if c[:3] not in acc]
                centre=(min(body)+max(body))/2
                check(abs(centre-16)<=1,f'Player {name} {f}: body centre {centre} drift')
        variation=(max(counts)-min(counts))/min(counts)
        check(variation<.25,f'Player {name}: pixel count varies {variation:.1%} ({min(counts)}..{max(counts)})')
        print(f'  player/{name}: {n} frames, opaque count {min(counts)}..{max(counts)}, variation {variation:.1%}')
    check(not opaque(cell(images['objects/dash_crystal.png'],7,16,16)),'Dash crystal frame 7 must be transparent')
    for name in ['enemies/walker.png','enemies/flyer.png','enemies/shooter.png','enemies/shot.png']:
        sh=sheets[name];cw,ch=sh['cell_width'],sh['cell_height'];a=images[name]
        for i in range(sh['columns']):
            px=opaque(cell(a,i,cw,ch))
            check(all(x not in (0,cw-1) and y!=0 for x,y,c in px),f'{name} {i}: forbidden border contact')
            if name!='enemies/shot.png':
                check(max(y for x,y,c in px)==ch-1,f'{name} {i}: bottom registration')
                centre=(min(x for x,y,c in px)+max(x for x,y,c in px))/2
                check(abs(centre-cw/2)<=1,f'{name} {i}: centre {centre}')
    # Directionally valid terrain pairs (top-to-top, side-to-side, concave joins).
    # Exposed borders are never incorrectly tested against an interior border.
    a=images['terrain.png'];tiles=[cell(a,i,16,16) for i in range(24)]
    edges=0
    fills=[6,7,9,14,15]
    pairs_h=[(0,1),(1,1),(1,2),(0,2),(16,17),(17,17),(17,18),(16,18),
             (20,21),(21,21),(21,22),(20,22),(1,4),(5,1),(17,12),(13,17)]
    pairs_v=[(0,8),(8,8),(8,16),(0,16),(2,10),(10,10),(10,18),(2,18),
             (3,11),(11,11),(11,19),(3,19),(8,4),(12,8),(10,5),(13,10)]
    for i in fills:
        pairs_h += [(8,i),(i,10),(5,i),(i,4),(13,i),(i,12)]
        pairs_v += [(1,i),(i,17),(12,i),(13,i),(i,4),(i,5)]
        for j in fills:pairs_h.append((i,j));pairs_v.append((i,j))
    for horizontal,pairs in [(True,pairs_h),(False,pairs_v)]:
        for i,j in pairs:
            edge1=[tiles[i].getpixel((15,v) if horizontal else (v,15)) for v in range(16)]
            edge2=[tiles[j].getpixel((0,v) if horizontal else (v,0)) for v in range(16)]
            check(edge1==edge2,f'Terrain {"horizontal" if horizontal else "vertical"} seam {i}->{j}')
            edges+=1
    mapping=json.loads((ROOT/'previews/terrain_test_map.json').read_text());cells={(x,y):i for x,y,i in mapping['cells']}
    seams=0
    room=Image.new('RGBA',(mapping['width']*16,mapping['height']*16),(0,0,0,0))
    for (x,y),i in cells.items():
        room.paste(tiles[i],(x*16,y*16))
        for dx,dy in [(1,0),(0,1)]:
            if (x+dx,y+dy) not in cells:continue
            other=tiles[cells[(x+dx,y+dy)]]
            e1=[tiles[i].getpixel((15,v) if dx else (v,15)) for v in range(16)]
            e2=[other.getpixel((0,v) if dx else (v,0)) for v in range(16)]
            check(e1==e2,f'Test room seam at {x,y}');seams+=1
    preview=images.get('previews/terrain_test.png') or Image.open(ROOT/'previews/terrain_test.png')
    expected=Image.new('RGBA',room.size,master[0]+(255,));expected.alpha_composite(room)
    check(list(preview.getdata())==list(expected.resize(preview.size,Image.Resampling.NEAREST).getdata()),'Terrain test preview differs from real tiles')
    print(f'  terrain: {edges} edge pair checks + {seams} assembled-room seams')
    for name in ['far','mid','near']:
        a=images[f'background/{name}.png']
        check([a.getpixel((0,y)) for y in range(224)]==[a.getpixel((399,y)) for y in range(224)],f'{name}: wrap edge mismatch')
        check(any(p[3]==0 for p in a.getdata()),f'{name}: missing transparent sky')
    spikes=images['objects/spikes.png']
    check(min(y for x,y,c in opaque(spikes))>=9,'Spikes exceed bottom 7px')
    check([spikes.getpixel((0,y)) for y in range(16)]==[spikes.getpixel((15,y)) for y in range(16)],'Spikes horizontal seam')
    gate=images['objects/gate_block.png']
    check([gate.getpixel((x,0)) for x in range(16)]==[gate.getpixel((x,15)) for x in range(16)],'Gate vertical seam')
    # Validate every preview retains hard pixels and the master palette.
    for p in (ROOT/'previews').glob('*.png'):
        a=Image.open(p).convert('RGBA')
        check(all(v[3] in [0,255] and (v[3]==0 or v[:3] in palette) for v in a.getdata()),f'{p.name}: invalid preview pixels')
        check(a.width%4==0 and a.height%4==0,f'{p.name}: not 4x')
        small=a.resize((a.width//4,a.height//4),Image.Resampling.NEAREST).resize(a.size,Image.Resampling.NEAREST)
        check(a.tobytes()==small.tobytes(),f'{p.name}: preview has non-nearest pixels')
    gifs=list((ROOT/'previews').glob('*.gif'))
    check(len(gifs)==sum(len(sh['animations']) for sh in sheets.values()),'Missing per-animation GIFs')
    for sh in sheets.values():
        for an in sh['animations']:
            p=ROOT/'previews'/(Path(sh['file']).stem+'_'+an['name']+'.gif');g=Image.open(p)
            check(g.size==(sh['cell_width']*4,sh['cell_height']*4),f'{p.name}: preview size')
            check(g.n_frames==an['frame_count'],f'{p.name}: frame count')
            check(('loop' in g.info)==an['loop'],f'{p.name}: GIF looping')
    if FAIL:
        print('\nFAIL: '+str(len(FAIL))+' failures')
        for msg in FAIL:print('  ERROR: '+msg)
        sys.exit(1)
    print(f'\nPASS: {CHECKS} checks; {len(EXPECTED)-1} native asset sheets; {len(palette)} shared colours; {len(gifs)} animation GIFs.')
    print('RGBA dimensions, binary alpha, palette exclusivity, registration, frame counts, terrain seams, parallax wraps and 4x previews verified.')
if __name__=='__main__':main()
