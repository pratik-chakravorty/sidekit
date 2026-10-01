"""Deterministic native-pixel authoring. Requires Pillow; no generated art inputs."""
from pathlib import Path
import json, math, random
from PIL import Image, ImageDraw
from PIL.PngImagePlugin import PngInfo

ROOT=Path(__file__).resolve().parent
P=['101322','1b2035','272c46','343a53','464c63','59647a','72859a','a5bcc4',
   '183c43','24575a','347776','56a39a','86c5ac','bcdfb5',
   '785537','ad7941','d9a351','f2cd7c','fff0bd',
   '682343','ad3265','ec5792','ff95b2',
   '423e70','555881','73769a','978fb1','d5c6cd',
   '6c302d','c45236','ff9861','302b36']
RGB=[tuple(bytes.fromhex(c)) for c in P]
ACC=[28,29,30]
SHEETS=[]
SRGB=PngInfo();SRGB.add(b'sRGB',b'\x00')
def im(w,h): return Image.new('RGBA',(w,h),(0,0,0,0))
def col(i): return RGB[i]+(255,)
def rect(a,box,c): ImageDraw.Draw(a).rectangle(box,fill=col(c))
def poly(a,pts,c): ImageDraw.Draw(a).polygon(pts,fill=col(c))
def line(a,pts,c,width=1): ImageDraw.Draw(a).line(pts,fill=col(c),width=width)
def dot(a,x,y,c):
    if 0<=x<a.width and 0<=y<a.height: a.putpixel((x,y),col(c))
def ell(a,box,c): ImageDraw.Draw(a).ellipse(box,fill=col(c))
def anim(name,start,count,fps=8,loop=True,row=None,pivot=None):
    v=dict(name=name,first_frame=start,frame_count=count,fps=fps,loop=loop)
    if row is not None:v['row']=row
    if pivot is not None:v['pivot']=pivot
    return v
def save_sheet(file,frames,cw,ch,anims=None,cols=None,rows=1,pivot=None):
    cols=cols or len(frames); out=im(cols*cw,rows*ch)
    for n,f in enumerate(frames):out.paste(f,((n%cols)*cw,(n//cols)*ch))
    dest=ROOT/file;dest.parent.mkdir(parents=True,exist_ok=True);out.save(dest,pnginfo=SRGB)
    SHEETS.append(dict(file=file,width=out.width,height=out.height,cell_width=cw,cell_height=ch,
                       columns=cols,rows=rows,pivot=pivot or [cw/2,ch],animations=anims or []))
    return out

def fox_head(a,cx,top,t,kind='idle',flat=False):
    """One original mountain-fox head model, reused and articulated in every pose."""
    x=cx-6
    # Two cropped triangular ears; the near ear is taller and angled forward.
    poly(a,[(x,top+5),(x,top+1),(x+2,top),(x+5,top+5)],0)
    poly(a,[(x+1,top+4),(x+1,top+2),(x+2,top+1),(x+4,top+4)],16)
    dot(a,x+2,top+3,18)
    poly(a,[(x+8,top+4),(x+9,top),(x+12,top),(x+12,top+5)],0)
    poly(a,[(x+9,top+3),(x+10,top+1),(x+11,top+1),(x+11,top+4)],17)
    dot(a,x+10,top+3,14)
    poly(a,[(x,top+4),(x+12,top+4),(x+13,top+7),(x+13,top+11),
            (x+10,top+13),(x+3,top+13),(x,top+10)],0)
    poly(a,[(x+1,top+5),(x+11,top+5),(x+12,top+8),(x+12,top+10),
            (x+9,top+12),(x+3,top+12),(x+1,top+9)],16)
    rect(a,(x+2,top+8,x+11,top+9),17)
    # Dark cheek and small off-centre pale muzzle establish a right-facing profile.
    rect(a,(x+1,top+9,x+3,top+10),15)
    rect(a,(x+7,top+10,x+12,top+11),18)
    dot(a,x+13,top+10,0);line(a,[(x+9,top+12),(x+11,top+12)],14)
    if kind=='hurt':
        line(a,[(x+4,top+8),(x+5,top+9)],0);line(a,[(x+9,top+8),(x+10,top+9)],0)
    elif kind=='idle' and t==6:
        line(a,[(x+3,top+9),(x+5,top+9)],0);line(a,[(x+8,top+9),(x+11,top+9)],0)
    else:
        rect(a,(x+3,top+8,x+5,top+8),18);dot(a,x+5,top+8,0)
        rect(a,(x+8,top+8,x+11,top+9),18);rect(a,(x+10,top+8,x+11,top+9),0)
    # Band and ties contain exactly the three reserved runtime-recolour colours.
    rect(a,(x-1,top+5,x+12,top+7),28)
    rect(a,(x,top+5,x+12,top+6),29)
    line(a,[(x+1,top+5),(x+5,top+5)],30);dot(a,x+9,top+5,30)
    wave=[0,1,1,0,-1,-1,0,1,0,-1,0,1][t%12]
    trailing=kind in ['run','dash']
    lift=-3 if kind=='fall' else 0
    length=6 if trailing else 4
    knot=(x-1,top+6)
    poly(a,[knot,(x-length,top+5+wave+lift),(x-length-1,top+7+wave+lift),(x-2,top+8)],28)
    line(a,[(x-length,top+6+wave+lift),(x-2,top+7)],29)
    line(a,[(x-2,top+7),(x-3,top+10+wave+lift)],28,2)
    dot(a,x-2,top+6,30)


def fox_limb(a,points,far=False):
    line(a,points,0,4);line(a,points,15 if far else 16,2)
    x,y=points[-1];line(a,[(x-1,y+1),(x+2,y+1)],0)
    line(a,[(x,y),(x+2,y)],15 if far else 17)


def player_pose(kind,t):
    a=im(32,32)
    if kind=='dash':
        # Reuse the same head at a lower registration, with a long low crouch.
        fox_limb(a,[(13,27),(9,29),(7,30)],True)
        fox_limb(a,[(17,27),(21,29),(23,30)])
        poly(a,[(8,24),(19,23),(22,28),(18,30),(10,29)],0)
        poly(a,[(10,25),(18,24),(20,28),(12,28)],16)
        line(a,[(12,27),(17,27)],18,2)
        # Squash the reusable head vertically by integer row selection only.
        head=im(32,32);fox_head(head,19,10,t,'dash')
        for yy,sy in enumerate([10,11,13,14,15,17,18,19,21,22,23]):
            for xx in range(32):
                c=head.getpixel((xx,sy))
                if c[3]:a.putpixel((xx,19+yy),c)
        fox_limb(a,[(19,28),(23,29),(25,30)])
        line(a,[(12,24),(9,23),(5+t//2,22+t%2)],28,2)
        line(a,[(12,24),(9,23),(5+t//2,22+t%2)],29)
        return a
    top=10
    if kind=='idle':top += 1 if t in [2,3,4] else 0
    if kind=='run':top += 1 if t in [1,2,7,8] else 0
    if kind in ['jump_rise','fall']:top=8+t%2
    if kind=='hurt':top=[10,11,11,10][t]
    cx=15 if kind=='hurt' and t<2 else 16
    if kind=='run':
        phase=2*math.pi*t/12; swing=math.sin(phase);stride=math.cos(phase)
        back=(15-round(5*stride),30-round(5*max(0,swing)))
        front=(17+round(5*stride),30-round(5*max(0,-swing)))
        fox_limb(a,[(14,27),(13-round(2*stride),28-round(3*max(0,swing))),back],True)
        fox_limb(a,[(18,27),(19+round(2*stride),28-round(3*max(0,-swing))),front])
        fox_limb(a,[(13,24),(12-round(3*swing),25),(13-round(4*swing),26)],True)
    elif kind in ['jump_rise','fall']:
        fox_limb(a,[(14,25),(11,27),(13,28-t%2)],True)
        fox_limb(a,[(18,25),(21,26),(20,29-t%2)])
    else:
        fox_limb(a,[(13,27),(12,29),(12,30)],True)
        fox_limb(a,[(19,27),(20,29),(20,30)])
    # Rounded pear-shaped torso; ivory belly, teal climbing belt, tiny buckle.
    poly(a,[(12,21),(20,21),(22,25),(21,28),(12,28),(11,25)],0)
    poly(a,[(13,22),(19,22),(21,25),(20,27),(13,27),(12,25)],16)
    rect(a,(15,23,19,26),18);dot(a,14,24,17)
    rect(a,(12,27,21,27),8);rect(a,(13,27,19,27),10);dot(a,17,27,17)
    if kind=='run':
        swing=math.sin(2*math.pi*t/12)
        fox_limb(a,[(20,23),(20+round(2*swing),25),(19+round(4*swing),26)])
    elif kind=='fall':
        fox_limb(a,[(12,24),(9,20),(9,16+t)],True)
        fox_limb(a,[(21,24),(24,20),(24,16+t)])
    elif kind=='wall_slide':
        fox_limb(a,[(20,24),(22,23),(22,19+t)])
        line(a,[(22,28),(23,30)],17)
    elif kind=='hurt':
        fox_limb(a,[(12,24),(9,23),(9,21+t%2)],True)
        fox_limb(a,[(20,24),(23,23),(23,21+t%2)])
    else:
        fox_limb(a,[(12,23),(10,25),(10,26)],True)
        fox_limb(a,[(20,23),(22,25),(22,26)])
    fox_head(a,cx,top,t,kind)
    return a

def build_player():
    specs=[('idle',8,8,True),('run',12,14,True),('jump_rise',2,10,True),('fall',2,10,True),
           ('dash',4,20,True),('wall_slide',2,6,True),('hurt',4,16,False),('appear',6,16,False)]
    frames=[];anims=[]
    for row,(name,n,fps,loop) in enumerate(specs):
        rowframes=[]
        for t in range(n):
            f=player_pose(name if name!='appear' else 'idle',t)
            if name=='appear' and t<5:
                pixels=[(x,y,f.getpixel((x,y))) for y in range(32) for x in range(32) if f.getpixel((x,y))[3]]
                out=im(32,32);used=set();rng=random.Random(700+t)
                for x,y,c in pixels:
                    radius=5-t
                    nx=max(1,min(30,x+rng.randint(-radius,radius)))
                    ny=max(1,min(31,y+rng.randint(-radius,radius)))
                    if (nx,ny) in used:
                        nx,ny=min(((xx,yy) for yy in range(1,32) for xx in range(1,31) if (xx,yy) not in used),key=lambda p:(p[0]-x)**2+(p[1]-y)**2)
                    used.add((nx,ny));out.putpixel((nx,ny),c)
                f=out
            rowframes.append(f)
        frames+=rowframes+[im(32,32) for _ in range(12-n)]
        anims.append(anim(name,row*12,n,fps,loop,row,[16,32]))
    save_sheet('player.png',frames,32,32,anims,12,8,[16,32])

QUIET=1
def terrain_tile(n=False,e=False,s=False,w=False,inner=None,variant=0):
    a=Image.new('RGBA',(16,16),col(QUIET))
    # Directional shared-edge signatures. Borders continue to the next cell;
    # concave corners carry the same profile into their corresponding two edges.
    if w:rect(a,(1,0,1,15),9);rect(a,(2,0,2,15),8)
    if e:rect(a,(14,0,14,15),8);rect(a,(13,0,13,15),2)
    if n:
        rect(a,(0,1,15,1),12);rect(a,(0,2,15,3),10);rect(a,(0,4,15,4),8)
        for x in [2,5,9,12]:dot(a,x,2,13)
        for x in [3,10]:dot(a,x,4,10);dot(a,x,5,8)
    if s:rect(a,(0,14,15,14),8);rect(a,(0,13,15,13),2)
    if inner:
        left='L' in inner;top='T' in inner
        x=0 if left else 15;y=0 if top else 15
        dx=1 if left else -1;dy=1 if top else -1
        side=[QUIET,9,8] if left else [QUIET,8,2]
        horiz=[QUIET,12,10,10,8] if top else [QUIET,8,2]
        # Rounded 4px return: top ledge turns smoothly into the vertical relief.
        for j,c in enumerate(horiz):
            for k in range(max(1,5-j)):dot(a,x+k*dx,y+j*dy,c)
        for k,c in enumerate(side):
            for j in range(max(1,4-k)):dot(a,x+k*dx,y+j*dy,c)
        # Copy canonical profiles last: these are the actual shared pixel rows.
        for j,c in enumerate(horiz):dot(a,x,y+j*dy,c)
        for k,c in enumerate(side):dot(a,x+k*dx,y,c)
        dot(a,x+dx,y+dy,11 if top else 8)
    # Interior variants keep a quiet perimeter and interchangeable low-contrast cuts.
    if not any([n,e,s,w,inner]):
        marks=[[(4,6),(5,6),(6,7)],[(10,10),(11,10)],[(6,12),(7,12)],[(9,4),(10,5)],[(3,9),(4,9)]]
        for x,y in marks[variant%5]:dot(a,x,y,2)
    return a

TILE_PARAMS=[(1,0,0,1,None),(1,0,0,0,None),(1,1,0,0,None),(1,1,0,1,None),
             (0,0,0,0,'TL'),(0,0,0,0,'TR'),(0,0,0,0,None),(0,0,0,0,None),
             (0,0,0,1,None),(0,0,0,0,None),(0,1,0,0,None),(0,1,0,1,None),
             (0,0,0,0,'BL'),(0,0,0,0,'BR'),(0,0,0,0,None),(0,0,0,0,None),
             (0,0,1,1,None),(0,0,1,0,None),(0,1,1,0,None),(0,1,1,1,None),
             (1,0,1,1,None),(1,0,1,0,None),(1,1,1,0,None),(1,1,1,1,None)]
TILE_NAMES=['TL-corner','Top','TR-corner','Single-column-top','InnerCorner-TL','InnerCorner-TR','Fill-A','Fill-B',
            'Left','Fill','Right','Single-column-mid','InnerCorner-BL','InnerCorner-BR','Fill-C','Fill-D',
            'BL-corner','Bottom','BR-corner','Single-column-bottom','Single-row-left','Single-row-mid','Single-row-right','Isolated']
def build_terrain():
    frames=[terrain_tile(*p,variant=i%5) for i,p in enumerate(TILE_PARAMS)]
    for t in range(8):
        a=im(16,16)
        if t<3:
            for x in [4,7,10]:line(a,[(x,14),(x-1+(t%2)*2,9-(x%3)),(x+1,6+(x%3))],10,1)
            line(a,[(3,14),(12,14)],9);dot(a,8,7,12)
        elif t<5:
            for x in [3,6,10]:line(a,[(x,1),(x,4+x%4),(x+1,5+x%4)],9 if t==3 else 10)
        elif t==5:line(a,[(3,4),(7,6),(6,9),(10,11)],3)
        elif t==6:
            line(a,[(8,14),(8,5)],9);poly(a,[(8,9),(3,6),(4,10)],11);poly(a,[(8,7),(12,3),(12,7)],12)
        else:
            line(a,[(6,14),(7,8)],10);rect(a,(5,6,9,8),16);dot(a,7,5,18)
        frames.append(a)
    save_sheet('terrain.png',frames,16,16,[],8,4)

def crystal(a,t,spent=False):
    poly(a,[(8,2),(12,6),(11,11),(8,14),(4,10),(3,6)],0)
    poly(a,[(8,3),(11,6),(10,10),(8,12),(5,9),(4,6)],5 if spent else 10)
    poly(a,[(8,3),(8,11),(5,8),(5,6)],6 if spent else 13)
    line(a,[(8,3),(10,6),(8,10)],7 if spent else 11)
    if not spent:dot(a,6+(t%3),5+(t%4),18)
def pickup(a,t):
    y=4+[0,0,1,1,0,-1][t%6]
    poly(a,[(8,y-2),(12,y+1),(12,y+6),(8,y+9),(4,y+6),(4,y+1)],0)
    poly(a,[(8,y-1),(11,y+1),(11,y+5),(8,y+7),(5,y+5),(5,y+1)],16)
    rect(a,(7,y+1,8,y+5),18);dot(a,9,y+2,15);dot(a,6,y+5,17)
    dot(a,6+t%3,y+1+t%4,17)
def objects():
    a=im(16,16);rect(a,(0,15,15,15),19)
    for x in [0,5,10]:poly(a,[(x,14),(x+2,9),(x+4,14)],20);line(a,[(x+2,10),(x+3,13)],22)
    for y in range(16):a.putpixel((15,y),a.getpixel((0,y)))
    save_sheet('objects/spikes.png',[a],16,16)
    frames=[]
    for t in range(4):
        a=im(32,32);pts=[]
        for i in range(32):
            theta=math.pi*2*i/32+math.pi*t/8;r=14 if i%2==0 else 10
            pts.append((round(16+r*math.cos(theta)),round(16+r*math.sin(theta))))
        poly(a,pts,19);ell(a,(7,7,25,25),20);ell(a,(9,9,23,23),4);ell(a,(12,12,20,20),1)
        for i in range(4):
            angle=t*math.pi/8+i*math.pi/2
            line(a,[(16,16),(round(16+9*math.cos(angle)),round(16+9*math.sin(angle)))],7,1)
        ell(a,(14,14,18,18),21);dot(a,16,15,22);frames.append(a)
    save_sheet('objects/saw.png',frames,32,32,[anim('rotate',0,4,16,pivot=[16,16])],pivot=[16,16])
    frames=[]
    for t,top in enumerate([7,11,3]):
        a=im(16,16);rect(a,(1,14,14,15),3);rect(a,(2,14,13,14),7)
        line(a,[(4,13),(10,top+3),(5,top+3),(11,13)],15,2)
        rect(a,(2,top,13,top+2),9);rect(a,(2,top,13,top),13);frames.append(a)
    save_sheet('objects/spring.png',frames,16,16,[anim('rest',0,1,1),anim('bounce',1,2,16,False)])
    frames=[]
    for t in range(8):
        a=im(16,16)
        if t<7:crystal(a,t,t==6)
        frames.append(a)
    save_sheet('objects/dash_crystal.png',frames,16,16,[anim('idle',0,6,10),anim('spent',6,1,1)],pivot=[8,8])
    frames=[]
    for t in range(6):a=im(16,16);pickup(a,t);frames.append(a)
    save_sheet('objects/collectible.png',frames,16,16,[anim('idle',0,6,8)],pivot=[8,8])
    frames=[]
    for t in range(7):
        a=im(16,32);rect(a,(5,30,10,31),3);rect(a,(7,4,8,29),6);dot(a,7,3,18)
        c=5 if t==0 else 16;wave=[0,1,2,1,0,-1,-1][t]
        poly(a,[(9,5),(14,6+wave),(13,11+wave),(9,10)],c);line(a,[(9,5),(13,6+wave)],7 if t==0 else 18)
        if t:dot(a,11,8+t%2,15)
        frames.append(a)
    save_sheet('objects/checkpoint.png',frames,16,32,[anim('inactive',0,1,1),anim('active',1,6,10)])
    frames=[]
    for t in range(6):
        a=im(32,32);rect(a,(5,29,26,31),3)
        poly(a,[(7,28),(7,10),(12,4),(20,4),(25,10),(25,28),(21,28),(21,11),(18,8),(14,8),(11,11),(11,28)],4)
        line(a,[(8,27),(8,11),(13,5),(19,5),(24,11),(24,27)],7)
        for y in [13,20]:rect(a,(7,y,10,y+1),15);rect(a,(22,y,25,y+1),15)
        if t:
            poly(a,[(16,11),(20,17),(16,25),(12,17)],9)
            line(a,[(16,13),(18,17),(16,22),(14,17),(16,13)],12)
            dot(a,16,14+t,18)
        else:rect(a,(14,19,17,24),5);dot(a,16,20,7)
        frames.append(a)
    save_sheet('objects/goal.png',frames,32,32,[anim('idle',0,1,1),anim('activated',1,5,10)])
    frames=[]
    for t in range(3):
        a=im(16,8);rect(a,(0,0,15,1),12);rect(a,(0,2,15,4),9);rect(a,(0,5,15,6),3)
        if t==0:rect(a,(0,2,1,6),7);dot(a,1,7,3)
        if t==2:rect(a,(14,2,15,6),7);dot(a,14,7,3)
        frames.append(a)
    save_sheet('objects/moving_platform.png',frames,16,8,[],pivot=[8,0])
    frames=[]
    for t in range(5):
        a=im(32,8)
        if t<3:
            rect(a,(0,0,31,1),12);rect(a,(0,2,31,5),9);rect(a,(0,6,31,7),3)
            for j in range(t):line(a,[(10+j*11,0),(12+j*8,3),(9+j*10,7)],0)
        else:
            for j in range(5):
                x=j*6+1;y=(j%2)*(2 if t==3 else 4)
                rect(a,(x,y,x+3,y+2),9);line(a,[(x,y),(x+3,y)],12)
        frames.append(a)
    save_sheet('objects/crumble_platform.png',frames,32,8,[anim('intact',0,1,1),anim('crumble',1,4,12,False)],pivot=[16,0])
    frames=[]
    for t in range(2):
        a=im(32,8);rect(a,(0,5,31,7),3);rect(a,(3,2+t*2,28,4+t*2),15)
        rect(a,(3,2+t*2,28,2+t*2),18);frames.append(a)
    save_sheet('objects/pressure_plate.png',frames,32,8,[anim('up',0,1,1),anim('pressed',1,1,1)],pivot=[16,8])
    a=im(16,16);rect(a,(0,0,15,15),1)
    for x in [2,7,12]:rect(a,(x,0,x+1,15),5);rect(a,(x,0,x,15),7)
    save_sheet('objects/gate_block.png',[a],16,16)
    frames=[]
    for t,height in enumerate([0,0,10,23,23,8]):
        a=im(32,32);rect(a,(2,27,29,31),3);rect(a,(3,27,28,28),6)
        for x in [4,12,20]:
            rect(a,(x,27,x+5,28),19 if t!=1 else 21)
            if height:
                poly(a,[(x,26),(x+3,27-height),(x+6,26)],20)
                line(a,[(x+3,29-height),(x+4,24)],22 if t==4 else 21)
        frames.append(a)
    save_sheet('objects/timed_spikes.png',frames,32,32,[anim('cycle',0,6,10)],pivot=[16,32])
    frames=[]
    for t in range(8):
        a=im(8,8)
        if t<4:
            r=3-t//2;ell(a,(4-r,4-r,4+r,4+r),5 if t<2 else 3)
            dot(a,2+t%2,6,4);dot(a,5,3,6 if t<2 else 4)
        else:
            r=[1,2,3,1][t-4];line(a,[(4-r,4),(4+r,4)],17);line(a,[(4,4-r),(4,4+r)],18);dot(a,4,4,18)
        frames.append(a)
    save_sheet('objects/particles.png',frames,8,8,[anim('dust',0,4,16,False),anim('sparkle',4,4,12,False)],pivot=[4,4])

def enemies():
    frames=[]
    for t in range(8):
        a=im(24,16)
        if t<6:
            rect(a,(6,13,9,15),3);rect(a,(15,13,18,15),3)
            if t%2:rect(a,(6,14,10,15),7)
            else:rect(a,(14,14,19,15),7)
            poly(a,[(5,12),(6,6),(10,3),(17,3),(20,7),(19,13)],0)
            poly(a,[(6,11),(7,6),(11,4),(17,4),(19,8),(18,12)],4)
            line(a,[(8,5),(16,5)],6);rect(a,(16,7,20,9),19);dot(a,19,7,22)
            line(a,[(7,9),(10,7),(12,10)],3)
            if t>=4:rect(a,(10,5,15,6),20);dot(a,12+t%2,5,21)
        else:
            rect(a,(4,12,20,15),0);rect(a,(5,13,19,14),4);rect(a,(15,13,18,13),20)
            if t==7:dot(a,9,12,6)
        frames.append(a)
    save_sheet('enemies/walker.png',frames,24,16,[anim('walk',0,4,10),anim('charge',4,2,16),anim('squashed',6,2,8,False)])
    frames=[]
    for t in range(8):
        a=im(24,16)
        if t<6:
            flap=[0,2,5,2,5,5][t]
            poly(a,[(10,8),(3,3+flap),(2,7+flap),(8,11),(11,12)],23)
            line(a,[(3,4+flap),(8,10),(10,9)],25)
            poly(a,[(14,8),(19,3+flap),(21,7+flap),(17,11),(13,12)],23)
            ell(a,(9,5,17,13),1);ell(a,(10,6,16,12),24)
            rect(a,(15,7,18,9),19);dot(a,17,7,22 if t<4 else 20)
            if t>=4:line(a,[(15,8),(17,8)],23);dot(a,12,9+t%2,25)
            rect(a,(10,14,12,15),6);rect(a,(15,14,17,15),6)
        else:
            poly(a,[(3,13),(10,11),(18,12),(21,15),(4,15)],23);rect(a,(12,13,17,14),24);dot(a,17,13,21)
            if t==7:dot(a,11,12,25)
        frames.append(a)
    save_sheet('enemies/flyer.png',frames,24,16,[anim('flap',0,4,12),anim('asleep',4,2,4),anim('squashed',6,2,8,False)])
    frames=[]
    for t in range(8):
        a=im(16,16)
        if t<7:
            rect(a,(3,14,12,15),3);poly(a,[(4,13),(3,7),(5,3),(10,3),(12,6),(12,13)],0)
            rect(a,(5,4,10,12),4);line(a,[(5,4),(10,4)],6)
            rect(a,(9,7,14,10),19);rect(a,(13,8,14,9),21 if t>=4 else 20)
            dot(a,7,6,21 if t==5 else 20)
            dot(a,6+t%2,5,6)
            if t>=4:line(a,[(5,10),(7,8)],20)
        else:rect(a,(2,13,13,15),4);rect(a,(9,13,13,14),20)
        frames.append(a)
    save_sheet('enemies/shooter.png',frames,16,16,[anim('idle',0,4,6),anim('shoot',4,3,12,False),anim('squashed',7,1,1,False)])
    frames=[]
    for t in range(2):
        a=im(8,8);poly(a,[(2,2),(5,2),(6,4),(5,6),(2,6),(1,4)],19);rect(a,(2,3,5,5),21);dot(a,4+t,3,22);frames.append(a)
    save_sheet('enemies/shot.png',frames,8,8,[anim('travel',0,2,12)],pivot=[4,4])

def backgrounds():
    a=im(8,224)
    for y0,y1,c in [(0,47,23),(48,95,24),(96,143,4),(144,183,3),(184,223,2)]:rect(a,(0,y0,7,y1),c)
    save_sheet('background/sky.png',[a],8,224,[],pivot=[0,0])
    for name,base,amp,cs in [('far',130,42,[3,4,24]),('mid',166,32,[2,3,4]),('near',202,19,[0,1,2])]:
        a=im(400,224)
        heights=[]
        for x in range(400):
            # Periodic sampled function including matching end columns.
            theta=2*math.pi*x/399
            h=round(base+amp*(.6*math.sin(theta+1)+.3*math.cos(theta*3)+.1*math.sin(theta*7)))
            heights.append(h);rect(a,(x,h,x,223),cs[0])
            rect(a,(x,min(223,h+12),x,223),cs[1])
        if name=='mid':
            # Ruined aqueduct: subdued solid geometry, sky openings remain transparent.
            for x in [48,112,176,240,304]:
                rect(a,(x,133,x+10,213),cs[0]);rect(a,(x,128,x+58,135),cs[0]);rect(a,(x+1,130,x+57,131),cs[2])
        elif name=='near':
            for x in [42,128,276,351]:
                h=heights[x];rect(a,(x,h-17,x+5,h+8),cs[0]);poly(a,[(x-9,h-7),(x+2,h-26),(x+15,h-8)],cs[0])
        # Structural decorations are away from wrapping columns.
        save_sheet(f'background/{name}.png',[a],400,224,[],pivot=[0,0])

def ui():
    a=im(48,48);rect(a,(0,0,47,47),0);rect(a,(1,1,46,46),7);rect(a,(3,3,44,44),8);rect(a,(5,5,42,42),1)
    for x,y in [(7,7),(40,7),(7,40),(40,40)]:
        poly(a,[(x,y-2),(x+2,y),(x,y+2),(x-2,y)],16);dot(a,x,y,18)
    save_sheet('ui/panel.png',[a],48,48,[],pivot=[24,24]);SHEETS[-1]['border']=[16,16,16,16]
    frames=[]
    for t in range(3):
        a=im(48,16);rect(a,(0,0,47,15),0);rect(a,(1,1,46,14),16 if t==1 else 6)
        rect(a,(3,3,44,12),8 if t!=2 else 1);rect(a,(4,3,43,3),11 if t!=2 else 3)
        frames.append(a)
    save_sheet('ui/button.png',frames,48,16,[anim('normal',0,1,1),anim('selected',1,1,1),anim('pressed',2,1,1)],pivot=[24,8]);SHEETS[-1]['border']=[8,8,8,8]
    a=im(96,80)
    poly(a,[(48,4),(80,45),(66,65),(30,65),(16,45)],0)
    line(a,[(48,5),(79,45),(65,64),(31,64),(17,45),(48,5)],16,2)
    poly(a,[(28,49),(48,19),(67,49),(58,49),(48,34),(38,49)],11)
    poly(a,[(41,48),(48,37),(55,48),(48,60)],17);line(a,[(48,40),(48,55)],18,2)
    for x in [25,68]:rect(a,(x,45,x+3,57),5);rect(a,(x-2,43,x+5,45),7)
    save_sheet('ui/logo.png',[a],96,80,[],pivot=[48,40])
    frames=[]
    for t in range(5):
        a=im(16,16)
        if t==0:crystal(a,0)
        elif t==1:pickup(a,0)
        elif t==2:ell(a,(2,2,13,13),7);ell(a,(3,3,12,12),1);line(a,[(8,4),(8,8),(11,9)],18)
        elif t==3:
            ell(a,(3,2,12,11),7);rect(a,(5,10,10,13),7);rect(a,(4,5,6,7),0);rect(a,(9,5,11,7),0);dot(a,8,9,0)
        else:rect(a,(4,3,6,12),7);rect(a,(9,3,11,12),7)
        frames.append(a)
    save_sheet('ui/icons.png',frames,16,16,[],pivot=[8,8]);SHEETS[-1]['frame_names']=['dash_crystal','collectible','clock','skull','pause']

def terrain_index(occupied,x,y):
    n=(x,y-1) not in occupied;e=(x+1,y) not in occupied;s=(x,y+1) not in occupied;w=(x-1,y) not in occupied
    lookup={(1,0,0,1):0,(1,0,0,0):1,(1,1,0,0):2,(1,1,0,1):3,(0,0,0,1):8,(0,0,0,0):9,(0,1,0,0):10,(0,1,0,1):11,
            (0,0,1,1):16,(0,0,1,0):17,(0,1,1,0):18,(0,1,1,1):19,(1,0,1,1):20,(1,0,1,0):21,(1,1,1,0):22,(1,1,1,1):23}
    mask=(int(n),int(e),int(s),int(w))
    if mask==(0,0,0,0):
        for dx,dy,idx in [(-1,-1,4),(1,-1,5),(-1,1,12),(1,1,13)]:
            if (x+dx,y+dy) not in occupied:return idx
        return [6,7,9,14,15][(x*7+y*3)%5]
    if mask in lookup:return lookup[mask]
    # Unlisted combinations (T-junction ends): compose exposed edge features.
    return None
def tile_at(sheet,index):return sheet.crop(((index%8)*16,(index//8)*16,(index%8+1)*16,(index//8+1)*16))
def render_room(occupied,w,h):
    a=im(w*16,h*16);ts=Image.open(ROOT/'terrain.png');mapping=[]
    for x,y in sorted(occupied):
        index=terrain_index(occupied,x,y)
        if index is None:raise ValueError(f'Test geometry requires absent terrain topology at {x,y}')
        a.paste(tile_at(ts,index),(x*16,y*16));mapping.append([x,y,index])
    return a,mapping
def block(o,x,y,w,h):o.update((xx,yy) for yy in range(y,y+h) for xx in range(x,x+w))
def previews():
    p=ROOT/'previews';p.mkdir(exist_ok=True)
    # Index 0 is transparency; palette indices 1..32 preserve exact colours.
    gifpal=[0,0,0]+[v for c in RGB for v in c]+[0]*(768-99)
    for sh in SHEETS:
        sheet=Image.open(ROOT/sh['file']);cw=sh['cell_width'];ch=sh['cell_height']
        for an in sh['animations']:
            frames=[]
            for i in range(an['frame_count']):
                index=an['first_frame']+i;x=index%sh['columns']*cw;y=index//sh['columns']*ch
                cell=sheet.crop((x,y,x+cw,y+ch)).resize((cw*4,ch*4),Image.Resampling.NEAREST)
                f=Image.new('P',cell.size);f.putpalette(gifpal)
                f.putdata([RGB.index(px[:3])+1 if px[3] else 0 for px in cell.getdata()]);frames.append(f)
            # GIF clocks have 10ms granularity; distribute timing error across frames.
            ticks=[round((i+1)*100/an['fps'])-round(i*100/an['fps']) for i in range(len(frames))]
            opts=dict(save_all=True,append_images=frames[1:],duration=[v*10 for v in ticks],transparency=0,disposal=2,optimize=False)
            if an['loop']:opts['loop']=0
            frames[0].save(p/(Path(sh['file']).stem+'_'+an['name']+'.gif'),**opts)
    occ=set();block(occ,1,2,7,5);block(occ,10,2,1,6);block(occ,13,2,6,1)
    block(occ,13,5,2,4);block(occ,15,7,4,2);block(occ,21,2,7,6)
    occ.difference_update((x,y) for y in range(2,4) for x in range(23,25))
    block(occ,2,10,7,3);block(occ,13,11,1,1)
    room,mapping=render_room(occ,30,15)
    backdrop=Image.new('RGBA',room.size,col(0));backdrop.alpha_composite(room)
    backdrop.resize((room.width*4,room.height*4),Image.Resampling.NEAREST).save(p/'terrain_test.png')
    (p/'terrain_test_map.json').write_text(json.dumps(dict(width=30,height=15,cells=mapping),indent=2))
    scene=im(400,224);sky=Image.open(ROOT/'background/sky.png')
    for x in range(0,400,8):scene.paste(sky,(x,0))
    for name in ['far','mid','near']:scene.alpha_composite(Image.open(ROOT/f'background/{name}.png'))
    o=set();block(o,0,12,25,2);block(o,2,9,5,3);block(o,12,8,5,2);block(o,20,5,4,2)
    # Bottom mass and discrete stepping islands leave clear gameplay lanes.
    r,_=render_room(o,25,14);scene.alpha_composite(r)
    def stamp(file,idx,cw,ch,x,y):
        sh=Image.open(ROOT/file);cols=sh.width//cw
        scene.alpha_composite(sh.crop((idx%cols*cw,idx//cols*ch,(idx%cols+1)*cw,(idx//cols+1)*ch)),(x,y))
    stamp('player.png',0,32,32,64,112)
    for x in [144,160,176]:stamp('objects/spikes.png',0,16,16,x,176)
    for idx,x in enumerate([128,144,160]):stamp('objects/moving_platform.png',idx,16,8,x,108)
    stamp('objects/dash_crystal.png',0,16,16,232,104)
    stamp('objects/collectible.png',0,16,16,300,64)
    stamp('objects/checkpoint.png',2,16,32,32,160)
    stamp('objects/goal.png',1,32,32,336,48)
    stamp('enemies/walker.png',0,24,16,256,176)
    stamp('terrain.png',24,16,16,48,128);stamp('terrain.png',30,16,16,208,112)
    scene.resize((1600,896),Image.Resampling.NEAREST).save(p/'scene_mockup.png')
    # Contact sheets aid silhouette/phase review without GIF playback support.
    player=Image.open(ROOT/'player.png');player.resize((1536,1024),Image.Resampling.NEAREST).save(p/'player_contact.png')
    run=im(12*32,48)
    rect(run,(0,0,383,47),0)
    for i in range(12):run.alpha_composite(player.crop((i*32,32,(i+1)*32,64)),(i*32,8))
    run.resize((1536,192),Image.Resampling.NEAREST).save(p/'run_contact.png')

def main():
    ROOT.mkdir(exist_ok=True)
    palette=im(32,1)
    for i in range(32):dot(palette,i,0,i)
    palette.save(ROOT/'palette.png',pnginfo=SRGB);(ROOT/'palette.hex').write_text('\n'.join('#'+c.upper() for c in P)+'\n')
    build_player();build_terrain();objects();enemies();backgrounds();ui()
    for sh in SHEETS:
        for an in sh['animations']:an.setdefault('pivot',sh['pivot'])
    manifest=dict(name='Aster Ridge',version='1.0.0',colour_space='sRGB',tile_size=16,
                  palette='palette.hex',player_accent_indices=ACC,pivot_coordinates='top-left origin, pixels; bottom edge = cell height',
                  import_settings=dict(filter='Point',compression='None',mipmaps=False,pixels_per_unit=16,alpha_is_transparency=True),
                  sheets=SHEETS,terrain_tile_names=TILE_NAMES+['grass_short','grass_tall','grass_bent','moss_drip','moss_long','crack','fern','amber_bloom'])
    (ROOT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    previews()
    print(f'Built {len(SHEETS)} native RGBA sheets and animation previews in {ROOT}')
if __name__=='__main__':main()
